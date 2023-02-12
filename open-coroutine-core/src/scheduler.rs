use crate::coroutine::{
    CoroutineResult, DefaultStack, OpenCoroutine, OpenYielder, ScopedCoroutine, Status, Yielder,
};
use crate::monitor::Monitor;
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use timer_utils::TimerList;
use uuid::Uuid;
use work_steal_queue::{LocalQueue, WorkStealQueue};

thread_local! {
    static YIELDER: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
    static TIMEOUT_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

/// 主协程
type MainCoroutine<'a> = ScopedCoroutine<'a, *mut Scheduler, (), (), DefaultStack>;

/// 用户协程
pub type SchedulableCoroutine =
    OpenCoroutine<'static, &'static mut c_void, (), &'static mut c_void>;

static mut SYSTEM_CALL_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

/// not support for now
static mut COPY_STACK_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

static mut SUSPEND_TABLE: Lazy<TimerList<SchedulableCoroutine>> = Lazy::new(TimerList::new);

static QUEUE: Lazy<WorkStealQueue<SchedulableCoroutine>> = Lazy::new(WorkStealQueue::default);

static mut RESULT_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler {
    name: &'static str,
    ready: LocalQueue<'static, SchedulableCoroutine>,
    scheduling: AtomicBool,
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        while !self.ready.is_empty() {
            self.try_schedule().unwrap();
        }
        assert!(
            self.ready.is_empty(),
            "there are still tasks to be carried out !"
        );
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler::with_name(&Uuid::new_v4().to_string())
    }

    pub fn with_name(name: &str) -> Self {
        Scheduler {
            name: Box::leak(Box::from(name)),
            ready: QUEUE.local_queue(),
            scheduling: AtomicBool::new(false),
        }
    }

    pub fn current<'a>() -> Option<&'a mut Scheduler> {
        if let Some(co) = SchedulableCoroutine::current() {
            if let Some(ptr) = co.get_scheduler() {
                return Some(unsafe { &mut *ptr });
            }
        }
        None
    }

    fn init_yielder(yielder: &Yielder<*mut Scheduler, ()>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *const _ as *const c_void;
        });
    }

    fn yielder() -> *const Yielder<*mut Scheduler, ()> {
        YIELDER.with(|boxed| unsafe { std::mem::transmute(*boxed.borrow_mut()) })
    }

    fn clean_yielder() {
        YIELDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null())
    }

    fn init_timeout_time(timeout_time: u64) {
        TIMEOUT_TIME.with(|boxed| {
            *boxed.borrow_mut() = timeout_time;
        });
    }

    fn timeout_time() -> u64 {
        TIMEOUT_TIME.with(|boxed| *boxed.borrow_mut())
    }

    fn clean_time() {
        TIMEOUT_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }

    pub fn submit<F>(
        &mut self,
        f: F,
        val: &'static mut c_void,
        size: usize,
    ) -> std::io::Result<&str>
    where
        F: FnOnce(
            &OpenYielder<&'static mut c_void, ()>,
            &'static mut c_void,
        ) -> &'static mut c_void,
        F: 'static,
    {
        let coroutine = SchedulableCoroutine::with_name(
            &(self.name.to_owned() + &Uuid::new_v4().to_string()),
            f,
            val,
            size,
        )?;
        coroutine.set_status(Status::Ready);
        coroutine.set_scheduler(self);
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.ready.push_back(coroutine);
        Ok(co_name)
    }

    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && unsafe { SUSPEND_TABLE.is_empty() && SYSTEM_CALL_TABLE.is_empty() }
    }

    pub fn timed_schedule(&mut self, timeout: Duration) -> std::io::Result<()> {
        let timeout_time = timer_utils::get_timeout_time(timeout);
        while !self.is_empty() {
            if timeout_time <= timer_utils::now() {
                break;
            }
            self.try_timeout_schedule(timeout_time)?;
        }
        Ok(())
    }

    pub fn try_schedule(&mut self) -> std::io::Result<()> {
        self.try_timeout_schedule(Duration::MAX.as_secs())
    }

    pub fn try_timed_schedule(&mut self, time: Duration) -> std::io::Result<()> {
        self.try_timeout_schedule(timer_utils::get_timeout_time(time))
    }

    pub fn try_timeout_schedule(&mut self, timeout_time: u64) -> std::io::Result<()> {
        if self
            .scheduling
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        Scheduler::init_timeout_time(timeout_time);
        let mut main = MainCoroutine::with_stack(
            DefaultStack::new(128 * 1024).map_err(|e| {
                self.scheduling.store(false, Ordering::Relaxed);
                e
            })?,
            |yielder: &Yielder<*mut Scheduler, ()>, scheduler: *mut Scheduler| {
                Scheduler::init_yielder(yielder);
                unsafe { (*scheduler).do_schedule() };
                unreachable!("should not execute to here !")
            },
        );
        assert_eq!(main.resume(self), CoroutineResult::Yield(()));
        Scheduler::clean_time();
        self.scheduling.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn back_to_main() {
        //跳回主线程
        let yielder = Scheduler::yielder();
        Scheduler::clean_yielder();
        if !yielder.is_null() {
            unsafe {
                (*yielder).suspend(());
            }
        }
    }

    pub(crate) fn do_schedule(&mut self) {
        if Scheduler::timeout_time() <= timer_utils::now() {
            Scheduler::back_to_main()
        }
        self.check_ready().unwrap();
        match self.ready.pop_front() {
            Some(coroutine) => {
                let start = timer_utils::get_timeout_time(Duration::from_millis(10));
                Monitor::add_task(start);
                //see OpenCoroutine::child_context_func
                match coroutine.resume() {
                    CoroutineResult::Yield(()) => {
                        if OpenYielder::<&'static mut c_void, ()>::syscall_flag() {
                            //syscall
                            unsafe {
                                SYSTEM_CALL_TABLE
                                    .insert(Box::leak(Box::from(coroutine.get_name())), coroutine);
                            }
                            OpenYielder::<&'static mut c_void, ()>::clean_syscall_flag();
                        } else {
                            let delay_time = OpenYielder::<&'static mut c_void, ()>::delay_time();
                            if delay_time > 0 {
                                //挂起协程到时间轮
                                unsafe {
                                    SUSPEND_TABLE.insert(
                                        timer_utils::add_timeout_time(delay_time),
                                        coroutine,
                                    );
                                }
                                OpenYielder::<&'static mut c_void, ()>::clean_delay();
                            } else {
                                //放入就绪队列尾部
                                self.ready.push_back(coroutine);
                            }
                        }
                    }
                    CoroutineResult::Return(_) => unreachable!("never have a result"),
                };
                self.do_schedule();
            }
            None => Scheduler::back_to_main(),
        }
    }

    fn check_ready(&mut self) -> std::io::Result<()> {
        unsafe {
            for _ in 0..SUSPEND_TABLE.len() {
                if let Some(entry) = SUSPEND_TABLE.front() {
                    let exec_time = entry.get_time();
                    if timer_utils::now() < exec_time {
                        break;
                    }
                    //移动至"就绪"队列
                    if let Some(mut entry) = SUSPEND_TABLE.pop_front() {
                        for _ in 0..entry.len() {
                            if let Some(coroutine) = entry.pop_front() {
                                coroutine.set_status(Status::Ready);
                                //把到时间的协程加入就绪队列
                                self.ready.push_back(coroutine);
                            }
                        }
                    }
                }
            }
            Ok(())
        }
    }

    pub(crate) fn resume(&mut self, co_name: usize) -> std::io::Result<()> {
        unsafe {
            let co_name = Box::leak(Box::new(std::ptr::read_unaligned(
                co_name as *const c_void as *const _ as *const String,
            )))
            .as_str();
            if let Some(co) = SYSTEM_CALL_TABLE.remove(co_name) {
                self.ready.push_back(co);
            }
        }
        Ok(())
    }

    pub(crate) fn save_result(co: SchedulableCoroutine) {
        unsafe {
            assert!(RESULT_TABLE
                .insert(Box::leak(Box::from(co.get_name())), co)
                .is_none())
        };
    }

    pub fn get_result(co_name: &'static str) -> Option<SchedulableCoroutine> {
        unsafe { RESULT_TABLE.remove(&co_name) }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn null() -> &'static mut c_void {
        unsafe { std::mem::transmute(10usize) }
    }

    #[test]
    fn simplest() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(
                |_yielder, _input| {
                    println!("[coroutine1] launched");
                    null()
                },
                null(),
                4096,
            )
            .expect("submit failed !");
        scheduler
            .submit(
                |_yielder, _input| {
                    println!("[coroutine2] launched");
                    null()
                },
                null(),
                4096,
            )
            .expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    #[test]
    fn with_suspend() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(
                |yielder, _input| {
                    println!("[coroutine1] suspend");
                    yielder.suspend(());
                    println!("[coroutine1] back");
                    null()
                },
                null(),
                4096,
            )
            .expect("submit failed !");
        scheduler
            .submit(
                |yielder, _input| {
                    println!("[coroutine2] suspend");
                    yielder.suspend(());
                    println!("[coroutine2] back");
                    null()
                },
                null(),
                4096,
            )
            .expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    fn delay(
    ) -> fn(&OpenYielder<&'static mut c_void, ()>, &'static mut c_void) -> &'static mut c_void {
        |yielder, _input| {
            println!("[coroutine] delay");
            yielder.delay((), 100);
            println!("[coroutine] back");
            null()
        }
    }

    #[test]
    fn with_delay() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(delay(), null(), 4096)
            .expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
        std::thread::sleep(Duration::from_millis(100));
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    #[test]
    fn timed_schedule() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(delay(), null(), 4096)
            .expect("submit failed !");
        scheduler
            .timed_schedule(Duration::from_millis(200))
            .expect("try_schedule failed !");
    }

    #[cfg(all(unix, feature = "preemptive-schedule"))]
    #[test]
    fn preemptive_schedule() -> std::io::Result<()> {
        use std::sync::{Arc, Condvar, Mutex};
        static mut TEST_FLAG: bool = true;
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::spawn(move || {
            let mut scheduler = Scheduler::new();
            scheduler
                .submit(
                    |_yielder, _input| {
                        unsafe {
                            while TEST_FLAG {
                                println!("loop");
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }
                        null()
                    },
                    null(),
                    4096,
                )
                .expect("submit failed !");
            scheduler
                .submit(
                    |_yielder, _input| {
                        unsafe {
                            TEST_FLAG = false;
                        }
                        null()
                    },
                    null(),
                    4096,
                )
                .expect("submit failed !");
            scheduler.try_schedule().expect("try_schedule failed !");

            let (lock, cvar) = &*pair2;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            // notify the condvar that the value has changed.
            cvar.notify_one();
        });

        // wait for the thread to start up
        let (lock, cvar) = &*pair;
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_millis(3000),
                |&mut pending| pending,
            )
            .unwrap();
        if result.1.timed_out() {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "preemptive schedule failed",
            ))
        } else {
            unsafe {
                handler.join().unwrap();
                assert!(!TEST_FLAG);
            }
            Ok(())
        }
    }
}
