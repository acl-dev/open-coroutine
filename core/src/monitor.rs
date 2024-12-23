use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, MONITOR_BEAN};
use crate::common::{get_timeout_time, now, CondvarBlocker};
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::coroutine::stack_pool::StackPool;
use crate::scheduler::SchedulableSuspender;
use crate::{catch, error, impl_current_for, impl_display_by_debug, info};
use nix::sys::pthread::{pthread_kill, pthread_self, Pthread};
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
    pthread: Pthread,
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
                // install SIGURG signal handler
                let mut set = SigSet::empty();
                set.add(Signal::SIGURG);
                let sa = SigAction::new(
                    SigHandler::Handler(sigurg_handler),
                    SaFlags::SA_RESTART,
                    set,
                );
                unsafe { _ = sigaction(Signal::SIGURG, &sa)? };
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
            //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
            for node in notify_queue {
                if now() < node.timestamp {
                    continue;
                }
                //实际上只对陷入重度计算的协程发送信号抢占
                //对于陷入执行系统调用的协程不发送信号(如果发送信号，会打断系统调用，进而降低总体性能)
                if pthread_kill(node.pthread, Signal::SIGURG).is_err() {
                    error!(
                        "Attempt to preempt scheduling for thread:{} failed !",
                        node.pthread
                    );
                }
            }
            StackPool::get_instance().clean();
            //monitor线程不执行协程计算任务，每次循环至少wait 1ms
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

    fn submit(timestamp: u64) -> std::io::Result<NotifyNode> {
        let instance = Self::get_instance();
        instance.start()?;
        let queue = unsafe { &mut *instance.notify_queue.get() };
        let node = NotifyNode {
            timestamp,
            pthread: pthread_self(),
        };
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

#[repr(C)]
#[derive(Debug, Default)]
pub(crate) struct MonitorListener {}

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
            | CoroutineState::SystemCall(_, _, _)
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
