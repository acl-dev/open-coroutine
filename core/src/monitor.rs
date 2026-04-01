use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, MONITOR_BEAN};
use crate::common::{get_timeout_time, now, CondvarBlocker};
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::scheduler::SchedulableSuspender;
use crate::{catch, error, impl_current_for, impl_display_by_debug, info};
use std::cell::{Cell, UnsafeCell};
use std::collections::HashSet;
use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        use nix::sys::pthread::{pthread_kill, pthread_self, Pthread};
        use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    } else if #[cfg(windows)] {
        use windows_sys::Win32::Foundation::{
            CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE,
        };
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentThread,
            SuspendThread, ResumeThread, GetThreadContext, SetThreadContext,
        };
        use windows_sys::Win32::System::Diagnostics::Debug::CONTEXT;
    }
}

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        #[repr(C)]
        #[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
        struct NotifyNode {
            timestamp: u64,
            pthread: Pthread,
        }
    } else if #[cfg(windows)] {
        #[repr(C)]
        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        struct NotifyNode {
            timestamp: u64,
            thread_handle: usize,
        }

        impl std::hash::Hash for NotifyNode {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.timestamp.hash(state);
                self.thread_handle.hash(state);
            }
        }

        impl PartialOrd for NotifyNode {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for NotifyNode {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.timestamp
                    .cmp(&other.timestamp)
                    .then_with(|| self.thread_handle.cmp(&other.thread_handle))
            }
        }
    }
}

/// Enums used to describe monitor state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum MonitorState {
    /// The monitor is created.
    Created,
    /// The monitor is running.
    Running,
    /// The monitor is stopping.
    Stopping,
    /// The monitor is stopped.
    Stopped,
}

impl_display_by_debug!(MonitorState);

/// The monitor impls.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Monitor {
    notify_queue: UnsafeCell<HashSet<NotifyNode>>,
    state: Cell<MonitorState>,
    thread: UnsafeCell<MaybeUninit<JoinHandle<()>>>,
    blocker: Arc<CondvarBlocker>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor {
            notify_queue: UnsafeCell::default(),
            state: Cell::new(MonitorState::Created),
            thread: UnsafeCell::new(MaybeUninit::uninit()),
            blocker: Arc::default(),
        }
    }
}

// On Windows, we use SuspendThread/GetThreadContext/SetThreadContext/ResumeThread
// to preempt coroutines, similar to how Go implements goroutine preemption on Windows.
// The assembly preempt function saves all registers, calls do_preempt() which suspends
// the coroutine, and restores all registers before returning to the interrupted code.
#[cfg(windows)]
#[no_mangle]
extern "C" fn do_preempt() {
    if let Some(suspender) = SchedulableSuspender::current() {
        suspender.suspend();
    }
}

// Assembly preempt stubs for Windows - saves/restores all registers around do_preempt()
#[cfg(all(windows, target_arch = "x86_64"))]
std::arch::global_asm!(
    ".globl preempt_asm",
    "preempt_asm:",
    // Save flags
    "pushfq",
    // Save all general-purpose registers
    "push rax",
    "push rcx",
    "push rdx",
    "push rbx",
    "push rbp",
    "push rsi",
    "push rdi",
    "push r8",
    "push r9",
    "push r10",
    "push r11",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    // Save XMM registers (16 * 16 = 256 bytes)
    "sub rsp, 256",
    "movups [rsp+0x00], xmm0",
    "movups [rsp+0x10], xmm1",
    "movups [rsp+0x20], xmm2",
    "movups [rsp+0x30], xmm3",
    "movups [rsp+0x40], xmm4",
    "movups [rsp+0x50], xmm5",
    "movups [rsp+0x60], xmm6",
    "movups [rsp+0x70], xmm7",
    "movups [rsp+0x80], xmm8",
    "movups [rsp+0x90], xmm9",
    "movups [rsp+0xa0], xmm10",
    "movups [rsp+0xb0], xmm11",
    "movups [rsp+0xc0], xmm12",
    "movups [rsp+0xd0], xmm13",
    "movups [rsp+0xe0], xmm14",
    "movups [rsp+0xf0], xmm15",
    // Save RSP and align stack for function call
    "mov rbx, rsp",
    "and rsp, -16",
    // Shadow space for Windows x64 calling convention
    "sub rsp, 32",
    "call do_preempt",
    // Restore RSP
    "mov rsp, rbx",
    // Restore XMM registers
    "movups xmm0,  [rsp+0x00]",
    "movups xmm1,  [rsp+0x10]",
    "movups xmm2,  [rsp+0x20]",
    "movups xmm3,  [rsp+0x30]",
    "movups xmm4,  [rsp+0x40]",
    "movups xmm5,  [rsp+0x50]",
    "movups xmm6,  [rsp+0x60]",
    "movups xmm7,  [rsp+0x70]",
    "movups xmm8,  [rsp+0x80]",
    "movups xmm9,  [rsp+0x90]",
    "movups xmm10, [rsp+0xa0]",
    "movups xmm11, [rsp+0xb0]",
    "movups xmm12, [rsp+0xc0]",
    "movups xmm13, [rsp+0xd0]",
    "movups xmm14, [rsp+0xe0]",
    "movups xmm15, [rsp+0xf0]",
    "add rsp, 256",
    // Restore general-purpose registers
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop r11",
    "pop r10",
    "pop r9",
    "pop r8",
    "pop rdi",
    "pop rsi",
    "pop rbp",
    "pop rbx",
    "pop rdx",
    "pop rcx",
    "pop rax",
    // Restore flags
    "popfq",
    "ret",
);

#[cfg(all(windows, target_arch = "x86"))]
std::arch::global_asm!(
    ".globl _preempt_asm",
    "_preempt_asm:",
    // Save flags
    "pushfd",
    // Save all general-purpose registers
    "push eax",
    "push ecx",
    "push edx",
    "push ebx",
    "push ebp",
    "push esi",
    "push edi",
    // Save XMM registers (8 * 16 = 128 bytes)
    "sub esp, 128",
    "movups [esp+0x00], xmm0",
    "movups [esp+0x10], xmm1",
    "movups [esp+0x20], xmm2",
    "movups [esp+0x30], xmm3",
    "movups [esp+0x40], xmm4",
    "movups [esp+0x50], xmm5",
    "movups [esp+0x60], xmm6",
    "movups [esp+0x70], xmm7",
    // Save ESP and align stack for function call
    "mov ebx, esp",
    "and esp, -16",
    "sub esp, 4",
    "call _do_preempt",
    // Restore ESP
    "mov esp, ebx",
    // Restore XMM registers
    "movups xmm0, [esp+0x00]",
    "movups xmm1, [esp+0x10]",
    "movups xmm2, [esp+0x20]",
    "movups xmm3, [esp+0x30]",
    "movups xmm4, [esp+0x40]",
    "movups xmm5, [esp+0x50]",
    "movups xmm6, [esp+0x60]",
    "movups xmm7, [esp+0x70]",
    "add esp, 128",
    // Restore general-purpose registers
    "pop edi",
    "pop esi",
    "pop ebp",
    "pop ebx",
    "pop edx",
    "pop ecx",
    "pop eax",
    // Restore flags
    "popfd",
    "ret",
);

#[cfg(all(windows, target_arch = "x86_64"))]
extern "C" {
    fn preempt_asm();
}

#[cfg(all(windows, target_arch = "x86"))]
extern "C" {
    #[link_name = "_preempt_asm"]
    fn preempt_asm();
}

impl Monitor {
    fn get_instance<'m>() -> &'m Self {
        BeanFactory::get_or_default(MONITOR_BEAN)
    }

    fn start(&self) -> std::io::Result<()> {
        match self.state.get() {
            MonitorState::Created => {
                self.state.set(MonitorState::Running);
                // install panic hook
                std::panic::set_hook(Box::new(|panic_hook_info| {
                    let syscall = crate::common::constants::SyscallName::panicking;
                    if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
                        let new_state = crate::common::constants::SyscallState::Executing;
                        if co.syscall((), syscall, new_state).is_err() {
                            error!(
                                "{} change to syscall {} {} failed !",
                                co.name(),
                                syscall,
                                new_state
                            );
                        }
                    }
                    eprintln!(
                        "panic hooked in open-coroutine, thread '{}' {}",
                        std::thread::current().name().unwrap_or("unknown"),
                        panic_hook_info
                    );
                    eprintln!(
                        "stack backtrace:\n{}",
                        std::backtrace::Backtrace::force_capture()
                    );
                    if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
                        if co.running().is_err() {
                            error!("{} change to running state failed !", co.name());
                        }
                    }
                }));
                #[cfg(unix)]
                {
                    // install SIGURG signal handler
                    extern "C" fn sigurg_handler(_: libc::c_int) {
                        if let Ok(mut set) = SigSet::thread_get_mask() {
                            set.remove(Signal::SIGURG);
                            set.thread_set_mask()
                                .expect("Failed to remove SIGURG signal mask!");
                            if let Some(suspender) = SchedulableSuspender::current() {
                                suspender.suspend();
                            }
                        }
                    }
                    let mut set = SigSet::empty();
                    set.add(Signal::SIGURG);
                    let sa = SigAction::new(
                        SigHandler::Handler(sigurg_handler),
                        SaFlags::SA_RESTART,
                        set,
                    );
                    unsafe { _ = sigaction(Signal::SIGURG, &sa)? };
                }
                // start the monitor thread
                let monitor = unsafe { &mut *self.thread.get() };
                *monitor = MaybeUninit::new(
                    std::thread::Builder::new()
                        .name("open-coroutine-monitor".to_string())
                        .spawn(|| {
                            info!("monitor started !");
                            if catch!(
                                Self::monitor_thread_main,
                                String::from("Monitor thread run failed without message"),
                                String::from("Monitor thread")
                            )
                            .is_ok()
                            {
                                info!("monitor stopped !");
                            }
                        })?,
                );
                Ok(())
            }
            MonitorState::Running => Ok(()),
            MonitorState::Stopping | MonitorState::Stopped => Err(Error::new(
                ErrorKind::Unsupported,
                "Restart operation is unsupported !",
            )),
        }
    }

    fn monitor_thread_main() {
        let monitor = Self::get_instance();
        Self::init_current(monitor);
        let notify_queue = unsafe { &*monitor.notify_queue.get() };
        while MonitorState::Running == monitor.state.get() || !notify_queue.is_empty() {
            for node in notify_queue {
                if now() < node.timestamp {
                    continue;
                }
                cfg_if::cfg_if! {
                    if #[cfg(unix)] {
                        if pthread_kill(node.pthread, Signal::SIGURG).is_err() {
                            error!(
                                "Attempt to preempt scheduling for thread:{} failed !",
                                node.pthread
                            );
                        }
                    } else if #[cfg(windows)] {
                        if !Self::preempt_thread(node.thread_handle as HANDLE) {
                            error!(
                                "Attempt to preempt scheduling for thread handle:{} failed !",
                                node.thread_handle
                            );
                        }
                    }
                }
            }
            monitor.blocker.clone().block(Duration::from_millis(1));
        }
        Self::clean_current();
        assert_eq!(
            MonitorState::Stopping,
            monitor.state.replace(MonitorState::Stopped)
        );
    }

    /// Preempt a thread on Windows by injecting a call to the preempt assembly stub.
    /// This is similar to Go's goroutine preemption on Windows using
    /// SuspendThread/GetThreadContext/SetThreadContext/ResumeThread.
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn preempt_thread(thread_handle: HANDLE) -> bool {
        unsafe {
            // Suspend the target thread
            if SuspendThread(thread_handle) == u32::MAX {
                return false;
            }
            let mut context: CONTEXT = std::mem::zeroed();
            // CONTEXT_FULL for x86_64 = 0x10000B
            context.ContextFlags = 0x10_000B;
            if GetThreadContext(thread_handle, &raw mut context) == 0 {
                ResumeThread(thread_handle);
                return false;
            }
            // Simulate a CALL instruction: push return address (original RIP) onto stack,
            // then set RIP to the preempt assembly function
            context.Rsp -= 8;
            std::ptr::write(context.Rsp as *mut u64, context.Rip);
            context.Rip = preempt_asm as u64;
            if SetThreadContext(thread_handle, &raw const context) == 0 {
                // Restore original RSP if SetThreadContext fails
                context.Rsp += 8;
                ResumeThread(thread_handle);
                return false;
            }
            ResumeThread(thread_handle);
            true
        }
    }

    /// Preempt a thread on Windows (x86/i686 variant).
    #[cfg(all(windows, target_arch = "x86"))]
    fn preempt_thread(thread_handle: HANDLE) -> bool {
        unsafe {
            if SuspendThread(thread_handle) == u32::MAX {
                return false;
            }
            let mut context: CONTEXT = std::mem::zeroed();
            // CONTEXT_FULL for x86 = 0x1000B
            context.ContextFlags = 0x1_000B;
            if GetThreadContext(thread_handle, &raw mut context) == 0 {
                ResumeThread(thread_handle);
                return false;
            }
            // Simulate a CALL instruction: push return address (original EIP) onto stack,
            // then set EIP to the preempt assembly function
            context.Esp -= 4;
            std::ptr::write(context.Esp as *mut u32, context.Eip);
            context.Eip = preempt_asm as u32;
            if SetThreadContext(thread_handle, &raw const context) == 0 {
                context.Esp += 4;
                ResumeThread(thread_handle);
                return false;
            }
            ResumeThread(thread_handle);
            true
        }
    }

    #[allow(dead_code)]
    pub(crate) fn stop() {
        Self::get_instance().state.set(MonitorState::Stopping);
    }

    fn submit(timestamp: u64) -> std::io::Result<NotifyNode> {
        let instance = Self::get_instance();
        instance.start()?;
        let queue = unsafe { &mut *instance.notify_queue.get() };
        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                let node = NotifyNode {
                    timestamp,
                    pthread: pthread_self(),
                };
            } else if #[cfg(windows)] {
                let node = unsafe {
                    let mut real_handle: HANDLE = 0;
                    let result = DuplicateHandle(
                        GetCurrentProcess(),
                        GetCurrentThread(),
                        GetCurrentProcess(),
                        &raw mut real_handle,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    );
                    if result == 0 {
                        return Err(Error::last_os_error());
                    }
                    NotifyNode {
                        timestamp,
                        thread_handle: real_handle as usize,
                    }
                };
            }
        }
        _ = queue.insert(node);
        instance.blocker.notify();
        Ok(node)
    }

    fn remove(node: &NotifyNode) -> bool {
        let instance = Self::get_instance();
        let queue = unsafe { &mut *instance.notify_queue.get() };
        let removed = queue.remove(node);
        #[cfg(windows)]
        if removed {
            unsafe {
                CloseHandle(node.thread_handle as HANDLE);
            }
        }
        removed
    }
}

impl_current_for!(MONITOR, Monitor);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct MonitorListener;

const NOTIFY_NODE: &str = "MONITOR_NODE";

impl<Yield, Return> Listener<Yield, Return> for MonitorListener {
    fn on_state_changed(
        &self,
        local: &CoroutineLocal,
        _: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>,
    ) {
        if Monitor::current().is_some() {
            return;
        }
        match new_state {
            CoroutineState::Ready => {}
            CoroutineState::Running => {
                let timestamp = get_timeout_time(Duration::from_millis(10));
                if let Ok(node) = Monitor::submit(timestamp) {
                    _ = local.put(NOTIFY_NODE, node);
                }
            }
            CoroutineState::Suspend(_, _)
            | CoroutineState::Syscall(_, _, _)
            | CoroutineState::Cancelled
            | CoroutineState::Complete(_)
            | CoroutineState::Error(_) => {
                if let Some(node) = local.get(NOTIFY_NODE) {
                    _ = Monitor::remove(node);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[cfg(not(target_arch = "riscv64"))]
    #[test]
    fn test() -> std::io::Result<()> {
        use nix::sys::pthread::pthread_kill;
        use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
        use std::os::unix::prelude::JoinHandleExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        static SIGNALED: AtomicBool = AtomicBool::new(false);
        extern "C" fn handler(_: libc::c_int) {
            SIGNALED.store(true, Ordering::Relaxed);
        }
        let mut set = SigSet::empty();
        set.add(Signal::SIGUSR1);
        let sa = SigAction::new(SigHandler::Handler(handler), SaFlags::SA_RESTART, set);
        unsafe { _ = sigaction(Signal::SIGUSR1, &sa)? };

        SIGNALED.store(false, Ordering::Relaxed);
        let handle = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
        });
        std::thread::sleep(Duration::from_secs(1));
        pthread_kill(handle.as_pthread_t(), Signal::SIGUSR1)?;
        std::thread::sleep(Duration::from_secs(2));
        assert!(SIGNALED.load(Ordering::Relaxed));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_suspend_resume_thread() -> std::io::Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;
        use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS};
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, GetCurrentThread, ResumeThread, SuspendThread,
        };

        static THREAD_RAN: AtomicBool = AtomicBool::new(false);

        let handle = std::thread::spawn(|| {
            // Get a real handle to the current thread
            let mut real_handle: usize = 0;
            unsafe {
                let result = DuplicateHandle(
                    GetCurrentProcess(),
                    GetCurrentThread(),
                    GetCurrentProcess(),
                    &raw mut real_handle as *mut _,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                );
                assert_ne!(result, 0);
            }
            THREAD_RAN.store(true, Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(2));
            unsafe {
                CloseHandle(real_handle as _);
            }
        });
        std::thread::sleep(Duration::from_secs(1));
        assert!(THREAD_RAN.load(Ordering::Relaxed));
        handle.join().expect("thread join failed");
        Ok(())
    }
}
