use crate::common::Current;
use crate::constants::{MonitorState, MONITOR_CPU};
use crate::monitor::node::NotifyNode;
#[cfg(feature = "net")]
use crate::net::event_loop::EventLoops;
use crate::scheduler::SchedulableSuspender;
use crate::{error, impl_current_for, warn};
use core_affinity::{set_for_current, CoreId};
use nix::sys::pthread::pthread_kill;
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use once_cell::sync::Lazy;
use open_coroutine_timer::now;
use std::cell::{Cell, UnsafeCell};
use std::collections::HashSet;
use std::io::{Error, ErrorKind};
use std::mem::MaybeUninit;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

mod node;

pub(crate) mod creator;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Monitor {
    cpu: usize,
    notify_queue: UnsafeCell<HashSet<NotifyNode>>,
    state: Cell<MonitorState>,
    thread: UnsafeCell<MaybeUninit<JoinHandle<()>>>,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.notify_queue.get_mut().is_empty(),
                "there are still timer tasks to be carried out !"
            );
            assert!(
                unsafe { self.thread.get_mut().assume_init_mut().is_finished() },
                "the monitor thread not finished !"
            );
        }
    }
}

impl_current_for!(MONITOR, Monitor);

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
        let notify_queue = unsafe { &*monitor.notify_queue.get() };
        while MonitorState::Running == monitor.state.get() || !notify_queue.is_empty() {
            //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
            for node in notify_queue {
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
                    _ = event_loop.wait_just(Some(Duration::from_millis(1)));
                    //push tasks to other event-loop
                    while let Some(task) = event_loop.pop() {
                        EventLoops::submit_raw(task);
                    }
                }
            }
        }
    }

    fn new() -> Self {
        //初始化monitor线程
        Monitor {
            cpu: MONITOR_CPU,
            notify_queue: UnsafeCell::default(),
            state: Cell::new(MonitorState::Created),
            thread: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    fn get_instance() -> &'static Monitor {
        unsafe { &*std::ptr::addr_of!(GLOBAL) }
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
                Monitor::register_handler(sigurg_handler);
                // start the monitor thread
                let monitor = unsafe { &mut *self.thread.get() };
                *monitor = MaybeUninit::new(
                    std::thread::Builder::new()
                        .name("open-coroutine-monitor".to_string())
                        .spawn(|| {
                            Monitor::init_current(Monitor::get_instance());
                            #[allow(unused_variables)]
                            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(
                                Monitor::monitor_thread_main,
                            )) {
                                #[cfg(feature = "logs")]
                                let message = *e
                                    .downcast_ref::<&'static str>()
                                    .unwrap_or(&"Monitor failed without message");
                                error!("open-coroutine-monitor exited with error:{}", message);
                            } else {
                                warn!("open-coroutine-monitor has exited");
                            }
                            Monitor::clean_current();
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

    fn submit(timestamp: u64) -> std::io::Result<NotifyNode> {
        let instance = Self::get_instance();
        instance.start()?;
        let queue = unsafe { &mut *instance.notify_queue.get() };
        let node = NotifyNode::new(timestamp);
        _ = queue.insert(node);
        Ok(node)
    }

    fn remove(node: &NotifyNode) -> bool {
        let instance = Self::get_instance();
        let queue = unsafe { &mut *instance.notify_queue.get() };
        queue.remove(node)
    }
}
