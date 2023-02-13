use crate::EventLoop;
use once_cell::sync::{Lazy, OnceCell};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

thread_local! {
    static SIGNAL_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

pub(crate) struct Monitor {
    #[cfg(all(unix, feature = "preemptive-schedule"))]
    task: timer_utils::TimerList<libc::pthread_t>,
    flag: AtomicBool,
}

unsafe impl Send for Monitor {}

unsafe impl Sync for Monitor {}

impl Monitor {
    #[cfg(unix)]
    fn register_handler(sigurg_handler: libc::sighandler_t) {
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler;
            libc::sigaddset(&mut act.sa_mask, libc::SIGURG);
            act.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGURG, &act, std::ptr::null_mut());
        }
    }

    fn new() -> Self {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        {
            extern "C" fn sigurg_handler(_signal: libc::c_int) {
                // invoke by Monitor::signal()
                let yielder = crate::OpenYielder::<&'static mut libc::c_void, ()>::yielder();
                if !yielder.is_null() {
                    //挂起当前协程
                    unsafe { (*yielder).suspend(()) };
                }
            }
            Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        }
        //通过这种方式来初始化monitor线程
        MONITOR.get_or_init(|| {
            std::thread::spawn(|| {
                let monitor = Monitor::global();
                while monitor.flag.load(Ordering::Acquire) {
                    #[cfg(all(unix, feature = "preemptive-schedule"))]
                    monitor.signal();
                    //尽量至少wait 1ms
                    let timeout_time = timer_utils::add_timeout_time(1_999_999);
                    let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
                }
            })
        });
        Monitor {
            #[cfg(all(unix, feature = "preemptive-schedule"))]
            task: timer_utils::TimerList::new(),
            flag: AtomicBool::new(true),
        }
    }

    fn global() -> &'static mut Monitor {
        unsafe { &mut GLOBAL }
    }

    /// 只在测试时使用
    pub(crate) fn stop() {
        Monitor::global().flag.store(false, Ordering::Release);
    }

    #[cfg(all(unix, feature = "preemptive-schedule"))]
    fn signal(&mut self) {
        //只遍历，不删除，如果抢占调度失败，会在1ms后不断重试，相当于主动检测
        for entry in self.task.iter() {
            let exec_time = entry.get_time();
            if timer_utils::now() < exec_time {
                break;
            }
            for p in entry.iter() {
                unsafe {
                    let pthread = std::ptr::read_unaligned(p);
                    libc::pthread_kill(pthread, libc::SIGURG);
                }
            }
        }
    }

    pub(crate) fn add_task(time: u64) {
        Monitor::init_signal_time(time);
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        unsafe {
            let pthread = libc::pthread_self();
            Monitor::global().task.insert(time, pthread);
        }
    }

    pub(crate) fn clean_task(_time: u64) {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        if let Some(entry) = Monitor::global().task.get_entry(_time) {
            unsafe {
                let pthread = libc::pthread_self();
                entry.remove(pthread);
            }
            Monitor::clean_signal_time();
        }
    }

    fn init_signal_time(time: u64) {
        SIGNAL_TIME.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn signal_time() -> u64 {
        SIGNAL_TIME.with(|boxed| *boxed.borrow_mut())
    }

    fn clean_signal_time() {
        SIGNAL_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }
}

#[cfg(all(test, unix, feature = "preemptive-schedule"))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg handled");
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        let time = timer_utils::get_timeout_time(Duration::from_millis(10));
        Monitor::add_task(time);
        std::thread::sleep(Duration::from_millis(20));
        Monitor::clean_task(time);
    }

    #[test]
    fn test_clean() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        let time = timer_utils::get_timeout_time(Duration::from_millis(500));
        Monitor::add_task(time);
        Monitor::clean_task(time);
        std::thread::sleep(Duration::from_millis(600));
    }

    #[test]
    fn test_sigmask() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        shield!();
        let time = timer_utils::get_timeout_time(Duration::from_millis(1000));
        Monitor::add_task(time);
        std::thread::sleep(Duration::from_millis(1100));
        Monitor::clean_task(time);
    }
}
