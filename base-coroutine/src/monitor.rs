use crate::EventLoop;
use once_cell::sync::{Lazy, OnceCell};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use timer_utils::TimerList;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

thread_local! {
    static SIGNAL_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

pub(crate) struct Monitor {
    task: TimerList,
    flag: AtomicBool,
}

unsafe impl Send for Monitor {}

unsafe impl Sync for Monitor {}

impl Monitor {
    fn new() -> Self {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        unsafe {
            extern "C" fn sigurg_handler(_signal: libc::c_int) {
                // invoke by Monitor::signal()
                let yielder = crate::Coroutine::<
                    &'static mut libc::c_void,
                    &'static mut libc::c_void,
                >::yielder();
                if !yielder.is_null() {
                    //挂起当前协程
                    unsafe { (*yielder).suspend(()) };
                }
            }
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler as libc::sighandler_t;
            libc::sigaddset(&mut act.sa_mask, libc::SIGURG);
            act.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGURG, &act, std::ptr::null_mut());
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
            task: TimerList::new(),
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
                    let pointer = std::ptr::read_unaligned(p);
                    let pthread =
                        std::ptr::read_unaligned(pointer as *mut _ as *mut libc::pthread_t);
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

    pub(crate) fn clean_task(time: u64) {
        if let Some(_entry) = Monitor::global().task.get_entry(time) {
            #[cfg(all(unix, feature = "preemptive-schedule"))]
            unsafe {
                let mut pthread = libc::pthread_self();
                _entry.remove_raw(&mut pthread as *mut _ as *mut libc::c_void);
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
    use crate::monitor::Monitor;
    use std::time::Duration;

    fn register_handler(sigurg_handler: libc::sighandler_t) {
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler;
            libc::sigaddset(&mut act.sa_mask, libc::SIGURG);
            act.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGURG, &act, std::ptr::null_mut());
        }
    }

    #[test]
    fn test_clean() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        register_handler(sigurg_handler as libc::sighandler_t);
        let time = timer_utils::get_timeout_time(Duration::from_millis(500));
        Monitor::add_task(time);
        Monitor::clean_task(time);
        std::thread::sleep(Duration::from_millis(600));
    }

    #[test]
    fn test() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg handled");
        }
        register_handler(sigurg_handler as libc::sighandler_t);
        Monitor::add_task(timer_utils::get_timeout_time(Duration::from_millis(10)));
        std::thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn test_sigmask() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        register_handler(sigurg_handler as libc::sighandler_t);
        shield!();
        Monitor::add_task(timer_utils::get_timeout_time(Duration::from_millis(1000)));
        std::thread::sleep(Duration::from_millis(1100));
    }
}
