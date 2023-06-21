use crate::coroutine::CoroutineState;
use crate::event_loop::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use once_cell::sync::{Lazy, OnceCell};
use open_coroutine_timer::TimerList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

struct TaskNode {
    pthread: libc::pthread_t,
    coroutine: Option<*const SchedulableCoroutine>,
}

impl Eq for TaskNode {}

impl PartialEq<Self> for TaskNode {
    fn eq(&self, other: &Self) -> bool {
        self.pthread.eq(&other.pthread)
    }
}

impl PartialOrd<Self> for TaskNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.pthread.partial_cmp(&other.pthread)
    }
}

impl Ord for TaskNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pthread.cmp(&other.pthread)
    }
}

pub(crate) struct Monitor {
    task: TimerList<TaskNode>,
    flag: AtomicBool,
}

impl Monitor {
    #[allow(dead_code)]
    pub(crate) fn signum() -> libc::c_int {
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux",
                         target_os = "l4re",
                         target_os = "android",
                         target_os = "emscripten"))] {
                libc::SIGRTMIN()
            } else {
                libc::SIGURG
            }
        }
    }

    #[allow(dead_code)]
    #[cfg(unix)]
    fn register_handler(sigurg_handler: libc::sighandler_t) {
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler;
            assert_eq!(0, libc::sigaddset(&mut act.sa_mask, Monitor::signum()));
            act.sa_flags = libc::SA_RESTART;
            assert_eq!(
                0,
                libc::sigaction(Monitor::signum(), &act, std::ptr::null_mut())
            );
        }
    }

    fn new() -> Self {
        #[allow(clippy::fn_to_numeric_cast)]
        unsafe extern "C" fn sigurg_handler(_signal: libc::c_int) {
            // invoke by Monitor::signal()
            if let Some(s) = crate::coroutine::suspender::Suspender::<(), ()>::current() {
                //获取当前信号屏蔽集
                let mut current_mask: libc::sigset_t = std::mem::zeroed();
                assert_eq!(
                    0,
                    libc::pthread_sigmask(libc::SIG_BLOCK, std::ptr::null(), &mut current_mask),
                );
                //删除对Monitor::signum()信号的屏蔽，使信号处理函数即使在处理中，也可以再次进入信号处理函数
                assert_eq!(0, libc::sigdelset(&mut current_mask, Monitor::signum()));
                assert_eq!(
                    0,
                    libc::pthread_sigmask(libc::SIG_SETMASK, &current_mask, std::ptr::null_mut())
                );
                s.suspend();
            }
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        //通过这种方式来初始化monitor线程
        _ = MONITOR.get_or_init(|| {
            std::thread::Builder::new()
                .name("open-coroutine-monitor".to_string())
                .spawn(|| {
                    let event_loop = EventLoops::monitor();
                    let monitor = Monitor::global();
                    while monitor.flag.load(Ordering::Acquire) {
                        monitor.signal();
                        //monitor线程不执行协程计算任务，每次循环至少wait 1ms
                        _ = event_loop.wait_just(Some(Duration::from_millis(1)));
                    }
                })
                .expect("failed to spawn monitor thread")
        });
        Monitor {
            task: TimerList::new(),
            flag: AtomicBool::new(true),
        }
    }

    fn global() -> &'static mut Monitor {
        unsafe { &mut GLOBAL }
    }

    pub fn stop() {
        Monitor::global().flag.store(false, Ordering::Release);
    }

    fn signal(&mut self) {
        //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
        for entry in self.task.iter() {
            let exec_time = entry.get_time();
            if open_coroutine_timer::now() < exec_time {
                break;
            }
            for node in entry.iter() {
                if let Some(coroutine) = node.coroutine {
                    unsafe {
                        if CoroutineState::Running == (*coroutine).get_state() {
                            //只对陷入重度计算的协程发送信号抢占，对陷入执行系统调用的协程
                            //不发送信号(如果发送信号，会打断系统调用，进而降低总体性能)
                            assert_eq!(0, libc::pthread_kill(node.pthread, Monitor::signum()));
                        }
                    }
                }
            }
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn add_task(time: u64, coroutine: Option<*const SchedulableCoroutine>) {
        unsafe {
            let pthread = libc::pthread_self();
            Monitor::global()
                .task
                .insert(time, TaskNode { pthread, coroutine });
        }
    }

    pub(crate) fn clean_task(time: u64) {
        if let Some(entry) = Monitor::global().task.get_entry(time) {
            unsafe {
                let pthread = libc::pthread_self();
                if !entry.is_empty() {
                    _ = entry.remove(&TaskNode {
                        pthread,
                        coroutine: None,
                    });
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[ignore]
    #[test]
    fn test() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg handled");
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        let time = open_coroutine_timer::get_timeout_time(Duration::from_millis(10));
        Monitor::add_task(time, None);
        std::thread::sleep(Duration::from_millis(20));
        Monitor::clean_task(time);
    }

    #[ignore]
    #[test]
    fn test_clean() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        let time = open_coroutine_timer::get_timeout_time(Duration::from_millis(100));
        Monitor::add_task(time, None);
        Monitor::clean_task(time);
        std::thread::sleep(Duration::from_millis(200));
    }
}
