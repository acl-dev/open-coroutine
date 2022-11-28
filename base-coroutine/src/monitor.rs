use once_cell::sync::{Lazy, OnceCell};
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use timer_utils::TimerList;

static mut GLOBAL: Lazy<ManuallyDrop<Monitor>> = Lazy::new(|| ManuallyDrop::new(Monitor::new()));

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
        //通过这种方式来初始化monitor线程
        MONITOR.get_or_init(|| {
            std::thread::spawn(|| {
                while Monitor::global().flag.load(Ordering::Acquire) {
                    Monitor::global().signal();
                    //fixme 这里在hook的情况下应该调用原始系统函数
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        });
        Monitor {
            task: TimerList::new(),
            flag: AtomicBool::new(true),
        }
    }

    fn global() -> &'static mut ManuallyDrop<Monitor> {
        unsafe { &mut GLOBAL }
    }

    /// 只在测试时使用
    pub(crate) fn stop() {
        Monitor::global().flag.store(false, Ordering::Release);
    }

    fn signal(&mut self) {
        while !self.task.is_empty() {
            self.do_signal();
        }
    }

    fn do_signal(&mut self) {
        for _ in 0..self.task.len() {
            if let Some(entry) = self.task.front() {
                let exec_time = entry.get_time();
                if timer_utils::now() < exec_time {
                    break;
                }
                if let Some(mut entry) = self.task.pop_front() {
                    for _ in 0..entry.len() {
                        if let Some(pointer) = entry.pop_front_raw() {
                            unsafe {
                                let pthread = std::ptr::read_unaligned(
                                    pointer as *mut _ as *mut libc::pthread_t,
                                );
                                libc::pthread_kill(pthread, libc::SIGURG);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn add_task(time: u64) {
        unsafe {
            let pthread = libc::pthread_self();
            Monitor::global().task.insert(time, pthread);
        }
    }

    pub(crate) fn clean_task(time: u64) {
        if let Some(entry) = Monitor::global().task.get_entry(time) {
            unsafe {
                let mut pthread = libc::pthread_self();
                entry.remove_raw(&mut pthread as *mut _ as *mut c_void);
            }
        }
    }

    pub(crate) fn init_signal_time(time: u64) {
        SIGNAL_TIME.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn signal_time() -> u64 {
        SIGNAL_TIME.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_signal_time() {
        SIGNAL_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::monitor::Monitor;
    use std::time::Duration;

    #[test]
    fn test() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg handled");
        }
        unsafe {
            libc::signal(libc::SIGURG, sigurg_handler as libc::sighandler_t);
            Monitor::add_task(timer_utils::get_timeout_time(Duration::from_millis(10)));
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn test_clean() {
        extern "C" fn sigurg_handler(_signal: libc::c_int) {
            println!("sigurg should not handle");
        }
        unsafe {
            libc::signal(libc::SIGURG, sigurg_handler as libc::sighandler_t);
            let time = timer_utils::get_timeout_time(Duration::from_millis(10));
            Monitor::add_task(time);
            Monitor::clean_task(time);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
