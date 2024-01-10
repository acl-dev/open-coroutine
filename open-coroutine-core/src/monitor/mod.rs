use crate::common::Current;
#[cfg(feature = "logs")]
use crate::common::Named;
use crate::constants::{CoroutineState, MONITOR_CPU};
use crate::coroutine::suspender::SimpleSuspender;
use crate::coroutine::StateCoroutine;
use crate::monitor::node::TaskNode;
use crate::net::event_loop::EventLoops;
use crate::pool::has::HasCoroutinePool;
use crate::pool::{CoroutinePool, CoroutinePoolImpl, TaskPool};
use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};
use core_affinity::{set_for_current, CoreId};
use nix::sys::pthread::pthread_kill;
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use once_cell::sync::Lazy;
use open_coroutine_timer::TimerList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

mod node;

pub(crate) mod creator;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Monitor {
    tasks: TimerList<TaskNode>,
    clean_queue: Vec<TaskNode>,
    thread: JoinHandle<()>,
    started: AtomicBool,
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.tasks.is_empty(),
                "there are still timer tasks to be carried out !"
            );
            assert!(
                self.thread.is_finished(),
                "the monitor thread not finished !"
            );
        }
    }
}

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

    fn new() -> Self {
        Monitor::register_handler(sigurg_handler);
        //通过这种方式来初始化monitor线程
        let thread = std::thread::Builder::new()
            .name("open-coroutine-monitor".to_string())
            .spawn(|| {
                // todo pin this thread to the CPU core closest to the network card
                if set_for_current(CoreId { id: MONITOR_CPU }) {
                    crate::warn!("pin monitor thread to CPU core-{MONITOR_CPU} failed !");
                }
                let pool = CoroutinePoolImpl::new(
                    String::from("open-coroutine-monitor"),
                    MONITOR_CPU,
                    crate::constants::DEFAULT_STACK_SIZE * 16,
                    1,
                    1,
                    0,
                    crate::common::DelayBlocker::default(),
                );
                let monitor = Monitor::global();
                while monitor.started.load(Ordering::Acquire) || !monitor.tasks.is_empty() {
                    {
                        //清理过期节点
                        let queue = &mut Monitor::global().clean_queue;
                        let tasks = &mut Monitor::global().tasks;
                        while let Some(node) = queue.pop() {
                            let timestamp = node.timestamp();
                            _ = tasks.remove(&timestamp, &node);
                        }
                    }
                    if !monitor.tasks.is_empty() {
                        //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
                        for (exec_time, entry) in &monitor.tasks {
                            if open_coroutine_timer::now() < *exec_time {
                                break;
                            }
                            if entry.is_empty() {
                                continue;
                            }
                            for node in entry {
                                _ = pool.submit(
                                    None,
                                    |_, _| {
                                        let coroutine = node.coroutine();
                                        if CoroutineState::Running == (*coroutine).state() {
                                            //只对陷入重度计算的协程发送信号抢占，对陷入执行系统调用的协程
                                            //不发送信号(如果发送信号，会打断系统调用，进而降低总体性能)
                                            if pthread_kill(node.pthread(), Signal::SIGURG).is_err() {
                                                crate::error!("Attempt to preempt scheduling for the coroutine:{} in thread:{} failed !",
                                                        coroutine.get_name(), node.pthread());
                                            }
                                        }
                                        None
                                    }, None);
                                _ = pool.try_schedule_task();
                            }
                        }
                    }
                    //monitor线程不执行协程计算任务，每次循环至少wait 1ms
                    let event_loop = EventLoops::monitor();
                    _ = event_loop.wait_just(Some(Duration::from_millis(1)));
                    //push tasks to other event-loop
                    while let Some(task) = event_loop.pop() {
                        EventLoops::submit_raw(task);
                    }
                }
                crate::warn!("open-coroutine-monitor has exited");
            })
            .expect("failed to spawn monitor thread");
        Monitor {
            tasks: TimerList::default(),
            clean_queue: Vec::default(),
            thread,
            started: AtomicBool::new(true),
        }
    }

    fn global() -> &'static mut Monitor {
        unsafe { &mut *std::ptr::addr_of_mut!(GLOBAL) }
    }

    pub fn stop() {
        Monitor::global().started.store(false, Ordering::Release);
        let pair = EventLoops::new_condition();
        let (lock, cvar) = pair.as_ref();
        let pending = lock.lock().unwrap();
        _ = pending.fetch_add(1, Ordering::Release);
        cvar.notify_one();
    }

    pub(crate) fn submit(time: u64, coroutine: &SchedulableCoroutine) {
        Monitor::global()
            .tasks
            .insert(time, TaskNode::new(time, coroutine));
    }

    pub(crate) fn remove(time: u64, coroutine: &SchedulableCoroutine) {
        let queue = &mut Monitor::global().clean_queue;
        queue.push(TaskNode::new(time, coroutine));
    }
}
