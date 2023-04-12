use once_cell::sync::{Lazy, OnceCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

pub(crate) struct Monitor {
    #[cfg(all(unix, feature = "preemptive-schedule"))]
    task: timer_utils::TimerList<libc::pthread_t>,
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
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        {
            #[allow(clippy::fn_to_numeric_cast)]
            unsafe extern "C" fn sigurg_handler(_signal: libc::c_int) {
                // invoke by Monitor::signal()
                if let Some(s) = crate::coroutine::suspender::Suspender::<(), ()>::current() {
                    //获取当前信号屏蔽集
                    let mut current_mask = libc::sigset_t::default();
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
                    s.suspend();
                }
            }
            Monitor::register_handler(sigurg_handler as libc::sighandler_t);
        }
        //通过这种方式来初始化monitor线程
        let _ = MONITOR.get_or_init(|| {
            std::thread::spawn(|| {
                let monitor = Monitor::global();
                while monitor.flag.load(Ordering::Acquire) {
                    #[cfg(all(unix, feature = "preemptive-schedule"))]
                    monitor.signal();
                    //尽量至少wait 1ms
                    std::thread::sleep(Duration::from_millis(1));
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
    #[allow(dead_code)]
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
                    assert_eq!(0, libc::pthread_kill(pthread, Monitor::signum()));
                }
            }
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn add_task(time: u64) {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        unsafe {
            let pthread = libc::pthread_self();
            Monitor::global().task.insert(time, pthread);
        }
    }

    #[allow(clippy::used_underscore_binding)]
    pub(crate) fn clean_task(_time: u64) {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        if let Some(entry) = Monitor::global().task.get_entry(_time) {
            unsafe {
                let pthread = libc::pthread_self();
                let _ = entry.remove(pthread);
            }
        }
    }
}

#[cfg(all(test, unix, feature = "preemptive-schedule"))]
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
        let time = timer_utils::get_timeout_time(Duration::from_millis(10));
        Monitor::add_task(time);
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
        let time = timer_utils::get_timeout_time(Duration::from_millis(100));
        Monitor::add_task(time);
        Monitor::clean_task(time);
        std::thread::sleep(Duration::from_millis(200));
    }

    // #[ignore]
    // #[test]
    // fn test_sigmask() {
    //     extern "C" fn sigurg_handler(_signal: libc::c_int) {
    //         println!("sigurg should not handle");
    //     }
    //     Monitor::register_handler(sigurg_handler as libc::sighandler_t);
    //     let _ = shield!();
    //     let time = timer_utils::get_timeout_time(Duration::from_millis(200));
    //     Monitor::add_task(time);
    //     std::thread::sleep(Duration::from_millis(300));
    //     Monitor::clean_task(time);
    // }
}
