use crate::common::Current;
use crate::constants::{MonitorState, MONITOR_CPU};
use crate::monitor::node::NotifyNode;
#[cfg(feature = "net")]
use crate::net::event_loop::EventLoops;
use crate::pool::TaskPool;
use crate::scheduler::SchedulableSuspender;
use crate::{error, impl_current_for, warn};
use core_affinity::{set_for_current, CoreId};
use nix::sys::pthread::pthread_kill;
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use once_cell::sync::Lazy;
use open_coroutine_timer::now;
use std::cell::Cell;
use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

mod node;

pub(crate) mod creator;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Monitor {
    cpu: usize,
    notify_queue: HashSet<NotifyNode>,
    state: Cell<MonitorState>,
    thread: JoinHandle<()>,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.notify_queue.is_empty(),
                "there are still timer tasks to be carried out !"
            );
            assert!(
                self.thread.is_finished(),
                "the monitor thread not finished !"
            );
        }
    }
}

impl_current_for!(MONITOR, Monitor);

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

impl Monitor {
    #[cfg(unix)]
    fn register_handler(sigurg_handler: extern "C" fn(libc::c_int)) {
        // install SIGURG signal handler
        let mut set = SigSet::empty();
        set.add(Signal::SIGURG);
        let sa = SigAction::new(
            SigHandler::Handler(sigurg_handler),
            SaFlags::SA_RESTART,
            set,
        );
        unsafe { _ = sigaction(Signal::SIGURG, &sa).unwrap() };
    }

    fn monitor_thread_main() {
        // todo pin this thread to the CPU core closest to the network card
        if set_for_current(CoreId { id: MONITOR_CPU }) {
            warn!("pin monitor thread to CPU core-{MONITOR_CPU} failed !");
        }
        let monitor = Monitor::get_instance();
        while MonitorState::Running == monitor.state.get() || !monitor.notify_queue.is_empty() {
            //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
            for node in &monitor.notify_queue {
                if now() < node.timestamp() {
                    continue;
                }
                //实际上只对陷入重度计算的协程发送信号抢占
                //对于陷入执行系统调用的协程不发送信号(如果发送信号，会打断系统调用，进而降低总体性能)
                if pthread_kill(node.pthread(), Signal::SIGURG).is_err() {
                    error!(
                        "Attempt to preempt scheduling for thread:{} failed !",
                        node.pthread()
                    );
                }
            }
            cfg_if::cfg_if! {
                if #[cfg(feature = "net")] {
                    //monitor线程不执行协程计算任务，每次循环至少wait 1ms
                    let event_loop = EventLoops::monitor();
                    _ = event_loop.wait_just(Some(std::time::Duration::from_millis(1)));
                    //push tasks to other event-loop
                    while let Some(task) = event_loop.pop() {
                        EventLoops::submit_raw(task);
                    }
                }
            }
        }
    }

    fn new() -> Self {
        Monitor::register_handler(sigurg_handler);
        //初始化monitor线程
        Monitor {
            cpu: MONITOR_CPU,
            notify_queue: HashSet::new(),
            state: Cell::new(MonitorState::Created),
            thread: std::thread::Builder::new()
                .name("open-coroutine-monitor".to_string())
                .spawn(|| {
                    Monitor::init_current(Monitor::get_instance());
                    #[allow(unused_variables)]
                    if let Err(e) =
                        std::panic::catch_unwind(AssertUnwindSafe(Monitor::monitor_thread_main))
                    {
                        #[cfg(feature = "logs")]
                        let message = *e
                            .downcast_ref::<&'static str>()
                            .unwrap_or(&"Monitor failed without message");
                        error!("open-coroutine-monitor exited with error:{}", message);
                    } else {
                        warn!("open-coroutine-monitor has exited");
                    }
                    Monitor::clean_current();
                })
                .expect("failed to spawn monitor thread"),
        }
    }

    fn get_instance() -> &'static mut Monitor {
        unsafe { &mut *std::ptr::addr_of_mut!(GLOBAL) }
    }

    #[allow(dead_code)]
    pub fn stop() {
        assert_eq!(
            MonitorState::Running,
            Self::get_instance().state.replace(MonitorState::Stopping)
        );
        cfg_if::cfg_if! {
            if #[cfg(feature = "net")] {
                let pair = EventLoops::new_condition();
                let (lock, cvar) = pair.as_ref();
                let pending = lock.lock().unwrap();
                _ = pending.fetch_add(1, Ordering::Release);
                cvar.notify_one();
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn submit(timestamp: u64) -> std::io::Result<NotifyNode> {
        let node = NotifyNode::new(timestamp);
        _ = Self::get_instance().notify_queue.insert(node);
        Ok(node)
    }

    fn remove(node: &NotifyNode) -> bool {
        Self::get_instance().notify_queue.remove(node)
    }
}
