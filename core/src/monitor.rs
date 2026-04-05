use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, MONITOR_BEAN};
use crate::common::{get_timeout_time, now, CondvarBlocker};
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::scheduler::SchedulableSuspender;
use crate::{catch, error, impl_current_for, impl_display_by_debug, info};
#[cfg(unix)]
use nix::sys::pthread::{pthread_kill, pthread_self, Pthread};
#[cfg(unix)]
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use std::cell::{Cell, UnsafeCell};
use std::collections::HashSet;
use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct NotifyNode {
    timestamp: u64,
    #[cfg(unix)]
    pthread: Pthread,
    #[cfg(windows)]
    thread_id: u32,
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

impl Monitor {
    fn get_instance<'m>() -> &'m Self {
        BeanFactory::get_or_default(MONITOR_BEAN)
    }

    fn start(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        extern "C" fn sigurg_handler(_: libc::c_int) {
            if let Ok(mut set) = SigSet::thread_get_mask() {
                //删除对SIGURG信号的屏蔽，使信号处理函数即使在处理中，也可以再次进入信号处理函数
                set.remove(Signal::SIGURG);
                set.thread_set_mask()
                    .expect("Failed to remove SIGURG signal mask!");
                if let Some(suspender) = SchedulableSuspender::current() {
                    suspender.suspend();
                }
            }
        }
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
        //先收集超时节点快照，再逐个检查是否仍在队列中
        //（在收集和检查之间，on_state_changed可能已将节点移除——协程进入了Syscall状态）
        let mut expired = Vec::new();
        while MonitorState::Running == monitor.state.get() || !notify_queue.is_empty() {
            //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
            expired.clear();
            let current = now();
            expired.extend(
                notify_queue
                    .iter()
                    .filter(|n| current >= n.timestamp)
                    .copied(),
            );
            for node in &expired {
                //实际上只对陷入重度计算的协程发送信号抢占
                //对于陷入执行系统调用的协程不发送信号(如果发送信号，会打断系统调用，进而降低总体性能)
                if !notify_queue.contains(node) {
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
                        // Two-level preemption: the first preempt_thread call
                        // sets a flag via do_preempt; the second call (~1ms
                        // later) forces suspension for CPU-bound coroutines.
                        // MonitorListener::on_state_changed removes the node
                        // when the coroutine transitions to Suspend.
                        if !Self::preempt_thread(node.thread_id) {
                            error!(
                                "Attempt to preempt scheduling for thread:{} failed !",
                                node.thread_id
                            );
                        }
                    }
                }
            }
            //monitor线程不执行协程计算任务，每次循环至少wait 1ms
            monitor.blocker.clone().block(Duration::from_millis(1));
        }
        Self::clean_current();
        assert_eq!(
            MonitorState::Stopping,
            monitor.state.replace(MonitorState::Stopped)
        );
    }

    #[cfg(windows)]
    fn preempt_thread(thread_id: u32) -> bool {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Diagnostics::Debug::CONTEXT;
        use windows_sys::Win32::System::Diagnostics::Debug::{GetThreadContext, SetThreadContext};
        use windows_sys::Win32::System::Threading::{
            OpenThread, ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_SET_CONTEXT,
            THREAD_SUSPEND_RESUME,
        };

        // Windows API requires CONTEXT to be 16-byte aligned for
        // GetThreadContext/SetThreadContext on x86_64. The windows-sys crate
        // defines CONTEXT with #[repr(C)] (natural alignment 8), which is
        // insufficient. This wrapper ensures proper alignment.
        #[cfg(target_arch = "x86_64")]
        #[repr(C, align(16))]
        struct AlignedContext(CONTEXT);

        extern "C" {
            fn preempt_asm();
        }

        unsafe {
            let handle = OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_SET_CONTEXT,
                0,
                thread_id,
            );
            if handle.is_null() {
                return false;
            }

            if SuspendThread(handle) == u32::MAX {
                _ = CloseHandle(handle);
                return false;
            }

            cfg_if::cfg_if! {
                if #[cfg(target_arch = "x86_64")] {
                    let mut aligned = AlignedContext(std::mem::zeroed());
                    let context = &mut aligned.0;
                    // CONTEXT_CONTROL for AMD64: captures/restores control
                    // registers (RIP, RSP, RBP, RFLAGS, segment registers).
                    // We only need to modify RIP and RSP to redirect the thread
                    // to preempt_asm. The assembly saves and restores all other
                    // registers (GPRs, XMMs, RFLAGS).
                    context.ContextFlags = 0x0010_0001;
                } else if #[cfg(target_arch = "x86")] {
                    let mut context: CONTEXT = std::mem::zeroed();
                    let context = &mut context;
                    // CONTEXT_CONTROL for i386
                    context.ContextFlags = 0x0001_0001;
                }
            }

            if GetThreadContext(handle, &raw mut *context) == 0 {
                _ = ResumeThread(handle);
                _ = CloseHandle(handle);
                return false;
            }

            cfg_if::cfg_if! {
                if #[cfg(target_arch = "x86_64")] {
                    // Push the do_preempt function pointer and original instruction
                    // pointer onto the thread's stack. preempt_asm will load the
                    // function pointer from the stack and call it indirectly, then
                    // RET to the original RIP.
                    // Safety: the target thread runs on a coroutine stack allocated
                    // by corosensei, which has ample space below the current RSP.
                    context.Rsp -= 16;
                    // [RSP+8] = return address (original RIP)
                    *((context.Rsp + 8) as *mut u64) = context.Rip;
                    // [RSP] = function pointer for preempt_asm to call
                    *(context.Rsp as *mut u64) = do_preempt as *const () as u64;
                    context.Rip = preempt_asm as *const () as u64;
                } else if #[cfg(target_arch = "x86")] {
                    // Push the do_preempt function pointer and original instruction
                    // pointer onto the thread's stack. preempt_asm will load the
                    // function pointer from the stack and call it indirectly, then
                    // RET to the original EIP.
                    // Safety: the target thread runs on a coroutine stack allocated
                    // by corosensei, which has ample space below the current ESP.
                    context.Esp -= 8;
                    // [ESP+4] = return address (original EIP)
                    *((context.Esp + 4) as *mut u32) = context.Eip;
                    // [ESP] = function pointer for preempt_asm to call
                    *(context.Esp as *mut u32) = do_preempt as *const () as u32;
                    context.Eip = preempt_asm as *const () as u32;
                }
            }

            if SetThreadContext(handle, &raw const *context) == 0 {
                _ = ResumeThread(handle);
                _ = CloseHandle(handle);
                return false;
            }

            _ = ResumeThread(handle);
            _ = CloseHandle(handle);
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
                let node = NotifyNode {
                    timestamp,
                    thread_id: unsafe {
                        windows_sys::Win32::System::Threading::GetCurrentThreadId()
                    },
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
        queue.remove(node)
    }
}

impl_current_for!(MONITOR, Monitor);

// Windows preemption: assembly stub that saves all registers, calls the
// preemption function pointer (passed on the stack by preempt_thread),
// restores registers, and returns to the original instruction pointer.
//
// Stack layout on entry:
//   [RSP]   = do_preempt function pointer
//   [RSP+8] = original RIP (return address)
//
// After all register saves, fn_ptr is at [saved_rsp + 384]:
//   256 (XMM0-XMM15) + 120 (15 GPRs: RAX,RCX,RDX,RBX,RBP,RSI,RDI,R8-R15)
//   + 8 (RFLAGS) = 384
#[cfg(all(windows, target_arch = "x86_64"))]
std::arch::global_asm!(
    ".globl preempt_asm",
    ".def preempt_asm",
    "  .scl 2",
    "  .type 32",
    ".endef",
    "preempt_asm:",
    "pushfq",
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
    "sub rsp, 256",
    "movups [rsp], xmm0",
    "movups [rsp+16], xmm1",
    "movups [rsp+32], xmm2",
    "movups [rsp+48], xmm3",
    "movups [rsp+64], xmm4",
    "movups [rsp+80], xmm5",
    "movups [rsp+96], xmm6",
    "movups [rsp+112], xmm7",
    "movups [rsp+128], xmm8",
    "movups [rsp+144], xmm9",
    "movups [rsp+160], xmm10",
    "movups [rsp+176], xmm11",
    "movups [rsp+192], xmm12",
    "movups [rsp+208], xmm13",
    "movups [rsp+224], xmm14",
    "movups [rsp+240], xmm15",
    "mov r12, rsp",
    // Align RSP to 16-byte boundary as required by Windows x64 ABI
    "and rsp, -16",
    // Allocate 32-byte shadow space for Windows x64 calling convention
    "sub rsp, 32",
    // Load fn_ptr from saved stack and call indirectly
    "mov rax, [r12 + 384]",
    "call rax",
    "mov rsp, r12",
    "movups xmm0, [rsp]",
    "movups xmm1, [rsp+16]",
    "movups xmm2, [rsp+32]",
    "movups xmm3, [rsp+48]",
    "movups xmm4, [rsp+64]",
    "movups xmm5, [rsp+80]",
    "movups xmm6, [rsp+96]",
    "movups xmm7, [rsp+112]",
    "movups xmm8, [rsp+128]",
    "movups xmm9, [rsp+144]",
    "movups xmm10, [rsp+160]",
    "movups xmm11, [rsp+176]",
    "movups xmm12, [rsp+192]",
    "movups xmm13, [rsp+208]",
    "movups xmm14, [rsp+224]",
    "movups xmm15, [rsp+240]",
    "add rsp, 256",
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
    "popfq",
    // Skip the fn_ptr slot, then RET pops original RIP
    "add rsp, 8",
    "ret",
);

// Stack layout on entry (i686):
//   [ESP]   = do_preempt function pointer
//   [ESP+4] = original EIP (return address)
//
// After all register saves, fn_ptr is at [saved_esp + 160]:
//   128 (XMM0-XMM7) + 28 (7 GPRs: EAX,ECX,EDX,EBX,EBP,ESI,EDI)
//   + 4 (EFLAGS) = 160
#[cfg(all(windows, target_arch = "x86"))]
std::arch::global_asm!(
    ".globl _preempt_asm",
    ".def _preempt_asm",
    "  .scl 2",
    "  .type 32",
    ".endef",
    "_preempt_asm:",
    "pushfd",
    "push eax",
    "push ecx",
    "push edx",
    "push ebx",
    "push ebp",
    "push esi",
    "push edi",
    "sub esp, 128",
    "movups [esp], xmm0",
    "movups [esp+16], xmm1",
    "movups [esp+32], xmm2",
    "movups [esp+48], xmm3",
    "movups [esp+64], xmm4",
    "movups [esp+80], xmm5",
    "movups [esp+96], xmm6",
    "movups [esp+112], xmm7",
    "mov ebx, esp",
    // Align ESP to 16-byte boundary for calling convention compliance
    "and esp, -16",
    // Load fn_ptr from saved stack and call indirectly
    "mov eax, [ebx + 160]",
    "call eax",
    "mov esp, ebx",
    "movups xmm0, [esp]",
    "movups xmm1, [esp+16]",
    "movups xmm2, [esp+32]",
    "movups xmm3, [esp+48]",
    "movups xmm4, [esp+64]",
    "movups xmm5, [esp+80]",
    "movups xmm6, [esp+96]",
    "movups xmm7, [esp+112]",
    "add esp, 128",
    "pop edi",
    "pop esi",
    "pop ebp",
    "pop ebx",
    "pop edx",
    "pop ecx",
    "pop eax",
    "popfd",
    // Skip the fn_ptr slot, then RET pops original EIP
    "add esp, 4",
    "ret",
);

// Thread-local flag for two-level preemption on Windows.
// Level 1: SuspendThread fires, do_preempt sets this flag and returns
//          without switching coroutines — the thread continues executing
//          and exits any critical section (heap allocation, IO, etc.).
//          If it reaches a hooked syscall, the Nio/Iocp layer will call
//          Suspender::suspend_with cooperatively.
// Level 2: If the flag is still set on the next SuspendThread (~1ms later),
//          the coroutine is truly CPU-bound with no syscalls — do_preempt
//          forces an immediate context switch.
#[cfg(windows)]
thread_local! {
    static PREEMPT_PENDING: Cell<bool> = const { Cell::new(false) };
}

#[cfg(windows)]
extern "C" fn do_preempt() {
    PREEMPT_PENDING.with(|flag| {
        if flag.get() {
            // Flag was already set from a previous SuspendThread attempt but the
            // coroutine never yielded (no hooked syscalls) — it is truly CPU-bound.
            // Force immediate suspension.
            flag.set(false);
            if let Some(suspender) = SchedulableSuspender::current() {
                suspender.suspend();
            }
        } else {
            // First attempt: set the flag and return without suspending.
            // preempt_asm will restore all registers and return to the original
            // code. This gives the thread time to exit any critical section.
            // If the coroutine reaches a hooked syscall, the Nio/Iocp layer
            // will yield cooperatively via Suspender::suspend_with.
            flag.set(true);
        }
    });
}

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
    fn test_preempt_thread() -> std::io::Result<()> {
        use crate::co;
        use crate::common::constants::CoroutineState;
        use crate::coroutine::Coroutine;
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        let pair = Arc::new((std::sync::Mutex::new(true), std::sync::Condvar::new()));
        let pair2 = pair.clone();
        let tid = Arc::new(AtomicU32::new(0));
        let tid2 = tid.clone();
        let co_running = Arc::new(AtomicBool::new(false));
        let co_running2 = co_running.clone();

        _ = std::thread::Builder::new()
            .name("test_preempt".to_string())
            .spawn(move || {
                tid2.store(
                    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
                    Ordering::Release,
                );
                let mut coroutine: Coroutine<(), (), ()> = co!(move |_, ()| {
                    co_running2.store(true, Ordering::Release);
                    loop {}
                })?;
                match coroutine.resume()? {
                    CoroutineState::Suspend((), 0) => {
                        assert_eq!(CoroutineState::Suspend((), 0), coroutine.state());
                    }
                    other => panic!("unexpected coroutine state: {other:?}"),
                }
                let (lock, cvar) = &*pair2;
                let mut pending = lock.lock().unwrap();
                *pending = false;
                cvar.notify_one();
                Ok::<(), std::io::Error>(())
            });

        // Wait for the coroutine to enter its loop
        while !co_running.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let thread_id = tid.load(Ordering::Acquire);
        assert_ne!(thread_id, 0, "Thread should have reported its ID");

        // Directly call preempt_thread to preempt the running coroutine.
        // Two-level preemption: the first call sets a cooperative flag (the
        // coroutine continues running), the second call forces suspension.
        assert!(
            super::Monitor::preempt_thread(thread_id),
            "preempt_thread should succeed (set cooperative flag)"
        );
        // Allow the first preempt_asm to complete before the second call
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            super::Monitor::preempt_thread(thread_id),
            "preempt_thread should succeed (force suspend)"
        );

        // Wait for thread to complete
        let (lock, cvar) = &*pair;
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_millis(3000),
                |&mut pending| pending,
            )
            .unwrap();
        if result.1.timed_out() {
            Err(std::io::Error::other(
                "preempt_thread did not preempt the coroutine",
            ))
        } else {
            Ok(())
        }
    }
}
