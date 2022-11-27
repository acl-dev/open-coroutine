use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::os::raw::c_void;
use std::thread::JoinHandle;
use std::time::Duration;
use timer_utils::TimerList;

static mut GLOBAL: Lazy<ManuallyDrop<Monitor>> = Lazy::new(|| ManuallyDrop::new(Monitor::new()));

static MONITOR: Lazy<JoinHandle<()>> = Lazy::new(|| {
    std::thread::spawn(|| {
        while Monitor::global().flag {
            Monitor::global().signal();
            //fixme 这里在hook的情况下应该调用原始系统函数
            std::thread::sleep(Duration::from_millis(1));
        }
    })
});

thread_local! {
    static SIGNAL_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

pub(crate) struct Monitor {
    task: TimerList,
    flag: bool,
}

unsafe impl Send for Monitor {}

impl Monitor {
    fn new() -> Self {
        //通过这种方式来初始化monitor线程
        let _t = MONITOR.thread();
        Monitor {
            task: TimerList::new(),
            flag: true,
        }
    }

    fn global() -> &'static mut ManuallyDrop<Monitor> {
        unsafe { &mut GLOBAL }
    }

    /// 只在测试时使用
    pub(crate) fn stop() {
        Monitor::global().flag = false;
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

    pub(crate) fn add_task(time: u64, pthread: libc::pthread_t) {
        Monitor::global().task.insert(time, pthread);
    }

    pub(crate) fn clean_task(time: u64, pthread: &mut libc::pthread_t) {
        if let Some(entry) = Monitor::global().task.get_entry(time) {
            entry.remove_raw(pthread as *mut _ as *mut c_void);
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
