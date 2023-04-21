use crate::EventLoop;
use once_cell::sync::{Lazy, OnceCell};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use timer_utils::TimerObjectList;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

thread_local! {
    static SIGNAL_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

pub struct Monitor {
    task: TimerObjectList,
    flag: AtomicBool,
}

unsafe impl Send for Monitor {}

unsafe impl Sync for Monitor {}

impl Monitor {
    #[cfg(unix)]
    pub fn signum() -> libc::c_int {
        // cfg_if::cfg_if! {
        //     if #[cfg(any(target_os = "linux",
        //                  target_os = "l4re",
        //                  target_os = "android",
        //                  target_os = "emscripten"))] {
        //         libc::SIGRTMIN()
        //     } else {
        //         libc::SIGURG
        //     }
        // }
        libc::SIGURG
    }

    #[cfg(unix)]
    fn register_handler(sigurg_handler: libc::sighandler_t) {
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler;
            assert_eq!(libc::sigaddset(&mut act.sa_mask, Monitor::signum()), 0);
            act.sa_flags = libc::SA_RESTART;
            assert_eq!(
                libc::sigaction(Monitor::signum(), &act, std::ptr::null_mut()),
                0
            );
        }
    }

    fn new() -> Self {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        {
            unsafe extern "C" fn sigurg_handler(_signal: libc::c_int) {
                // invoke by Monitor::signal()
                let yielder = crate::Coroutine::<
                    &'static mut libc::c_void,
                    &'static mut libc::c_void,
                >::yielder();
                if !yielder.is_null() {
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
                        libc::pthread_sigmask(
                            libc::SIG_SETMASK,
                            &current_mask,
                            std::ptr::null_mut()
                        )
                    );
                    //挂起当前协程
                    (*yielder).suspend(());
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
            task: TimerObjectList::new(),
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
