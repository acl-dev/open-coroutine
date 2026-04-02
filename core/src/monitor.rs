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
        use std::sync::atomic::{AtomicBool, Ordering};
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
        #[derive(Debug, Clone)]
        struct NotifyNode {
            timestamp: u64,
            preempt_flag: Arc<AtomicBool>,
        }

        impl Eq for NotifyNode {}

        impl PartialEq for NotifyNode {
            fn eq(&self, other: &Self) -> bool {
                self.timestamp == other.timestamp
                    && Arc::ptr_eq(&self.preempt_flag, &other.preempt_flag)
            }
        }

        impl std::hash::Hash for NotifyNode {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.timestamp.hash(state);
                Arc::as_ptr(&self.preempt_flag).hash(state);
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
                    .then_with(|| {
                        Arc::as_ptr(&self.preempt_flag).cmp(&Arc::as_ptr(&other.preempt_flag))
                    })
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
                        node.preempt_flag.store(true, Ordering::Release);
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

    #[allow(dead_code)]
    pub(crate) fn stop() {
        Self::get_instance().state.set(MonitorState::Stopping);
    }

    /// On Windows, check the preempt flag and suspend the current coroutine if set.
    /// On Unix, this is a no-op since preemption is handled asynchronously via SIGURG.
    #[cfg(windows)]
    pub(crate) fn check_preempt() {
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            if let Some(flag) = co.local().get::<Arc<AtomicBool>>(PREEMPT_FLAG) {
                if flag.load(Ordering::Acquire) {
                    if let Some(suspender) = SchedulableSuspender::current() {
                        suspender.suspend();
                    }
                }
            }
        }
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
                _ = queue.insert(node);
            } else if #[cfg(windows)] {
                let node = NotifyNode {
                    timestamp,
                    preempt_flag: Arc::new(AtomicBool::new(false)),
                };
                _ = queue.insert(node.clone());
            }
        }
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

#[cfg(windows)]
const PREEMPT_FLAG: &str = "MONITOR_PREEMPT_FLAG";

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
                    #[cfg(windows)]
                    {
                        _ = local.put(PREEMPT_FLAG, node.preempt_flag.clone());
                    }
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

}
