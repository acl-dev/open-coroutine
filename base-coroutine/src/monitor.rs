use crate::work_steal::{WorkStealQueue, GLOBAL_QUEUE, LOCAL_QUEUES};
use crate::{Coroutine, EventLoop};
use once_cell::sync::{Lazy, OnceCell};
use std::cell::RefCell;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use timer_utils::TimerList;

static mut GLOBAL: Lazy<Monitor> = Lazy::new(Monitor::new);

static MONITOR: OnceCell<JoinHandle<()>> = OnceCell::new();

thread_local! {
    static SIGNAL_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

//todo 支持主动检测
pub(crate) struct Monitor {
    task: TimerList,
    flag: AtomicBool,
}

unsafe impl Send for Monitor {}

unsafe impl Sync for Monitor {}

impl Monitor {
    fn new() -> Self {
        unsafe {
            extern "C" fn sigurg_handler(_signal: libc::c_int) {
                // invoke by Monitor::signal()
                let yielder = Coroutine::<&'static mut c_void, &'static mut c_void>::yielder();
                if !yielder.is_null() {
                    //挂起当前协程
                    unsafe { (*yielder).suspend(()) };
                }
            }
            #[cfg(not(windows))]
            {
                let mut act: libc::sigaction = std::mem::zeroed();
                act.sa_sigaction = sigurg_handler as libc::sighandler_t;
                libc::sigaddset(&mut act.sa_mask, libc::SIGURG);
                act.sa_flags = libc::SA_RESTART;
                libc::sigaction(libc::SIGURG, &act, std::ptr::null_mut());
            }
            #[cfg(windows)]
            libc::signal(libc::SIGINT, sigurg_handler as libc::sighandler_t);
        }
        //通过这种方式来初始化monitor线程
        MONITOR.get_or_init(|| {
            std::thread::spawn(|| {
                let monitor = Monitor::global();
                while monitor.flag.load(Ordering::Acquire) {
                    monitor.signal();
                    monitor.balance();
                    let _ = EventLoop::next().wait(Some(Duration::from_millis(1)));
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
                                #[cfg(not(windows))]
                                libc::pthread_kill(pthread, libc::SIGURG);
                                #[cfg(windows)]
                                libc::raise(libc::SIGINT);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn add_task(time: u64) {
        Monitor::init_signal_time(time);
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
            Monitor::clean_signal_time();
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

    fn balance(&self) {
        unsafe {
            if let Some(local_queues) = LOCAL_QUEUES.get_mut() {
                let mut max = (usize::MIN, 0);
                let mut min = (usize::MAX, 0);
                //全局队列没有则不从全局队列steal
                if !GLOBAL_QUEUE.is_empty() {
                    for i in 0..local_queues.len() {
                        let local_queue = local_queues.get_mut(i).unwrap();
                        if local_queue.spare() >= local_queue.capacity() * 3 / 4 {
                            //任务不多(count<=64)，先尝试从全局队列steal
                            if local_queue.try_lock() {
                                if WorkStealQueue::try_global_lock() {
                                    local_queue.steal_global(local_queue.capacity() / 4);
                                }
                                local_queue.release_lock();
                            }
                        }
                        let spare = local_queue.spare();
                        //find max
                        if spare > max.0 {
                            max.0 = spare;
                            max.1 = i;
                        }
                        //find min
                        if spare < min.0 {
                            min.0 = spare;
                            min.1 = i;
                        }
                    }
                }
                //任务少的从任务多的steal，相差不大时不steal
                if let Some(count) = max.0.checked_sub(min.0) {
                    if count >= 64 {
                        let idle_more = local_queues.get_mut(max.1).unwrap();
                        let idle_less = LOCAL_QUEUES.get_mut().unwrap().get_mut(min.1).unwrap();
                        if idle_more.try_lock() {
                            let _ = idle_more.steal_siblings(idle_less, count / 2);
                            idle_more.release_lock();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
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
        let time = timer_utils::get_timeout_time(Duration::from_millis(10));
        Monitor::add_task(time);
        Monitor::clean_task(time);
        std::thread::sleep(Duration::from_millis(20));
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
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigaddset(&mut set, libc::SIGURG);
            libc::pthread_sigmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
        }
        Monitor::add_task(timer_utils::get_timeout_time(Duration::from_millis(10)));
        std::thread::sleep(Duration::from_millis(20));
    }
}
