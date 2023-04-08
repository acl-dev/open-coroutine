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
            // SA_NODEFER：默认情况下，当信号函数运行时，内核将阻塞（不可重入）给定的信号，
            // 直至当次处理完毕才开始下一次的信号处理。但是设置该标记之后，那么信号函数
            // 将不会被阻塞，此时需要注意函数的可重入安全性。
            act.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART | libc::SA_NODEFER;
            assert_eq!(
                0,
                libc::sigaction(Monitor::signum(), &act, std::ptr::null_mut())
            );
        }
    }

    fn new() -> Self {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        {
            extern "C" fn yields() {
                if let Some(s) = crate::coroutine::suspender::Suspender::<(), ()>::current() {
                    s.suspend();
                }
            }
            #[allow(clippy::fn_to_numeric_cast)]
            unsafe extern "C" fn sigurg_handler(
                _signal: libc::c_int,
                _siginfo: &libc::siginfo_t,
                context: &mut libc::ucontext_t,
            ) {
                // invoke by Monitor::signal()
                // 在本方法结束后调用yields
                cfg_if::cfg_if! {
                    if #[cfg(all(
                        any(target_os = "linux", target_os = "android"),
                        target_arch = "x86_64",
                    ))] {
                        context.uc_mcontext.gregs[libc::REG_RIP as usize] = yields as i64;
                    } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                target_arch = "x86",
                    ))] {
                        context.uc_mcontext.gregs[libc::REG_EIP as usize] = yields as i32;
                    } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                target_arch = "aarch64",
                    ))] {
                        context.uc_mcontext.pc = yields as libc::c_ulong;
                    } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                target_arch = "arm",
                    ))] {
                        context.uc_mcontext.arm_pc = yields as libc::c_ulong;
                    } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                any(target_arch = "riscv64", target_arch = "riscv32"),
                    ))] {
                        context.uc_mcontext.__gregs[libc::REG_PC] = yields as libc::c_ulong;
                    } else if #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))] {
                        (*context.uc_mcontext).__ss.__pc = yields as u64;
                    } else if #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))] {
                        (*context.uc_mcontext).__ss.__rip = yields as u64;
                    } else {
                        compile_error!("Unsupported platform");
                    }
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
