use crate::coroutine::{Coroutine, CoroutineResult, OpenCoroutine, Status, UserFunc, Yielder};
use crate::id::IdGenerator;
#[cfg(unix)]
use crate::monitor::Monitor;
use crate::stack::{Stack, StackError};
use crate::work_steal::{get_queue, WorkStealQueue};
use object_collection::{ObjectList, ObjectMap};
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::os::raw::c_void;
use std::time::Duration;
use timer_utils::TimerList;

thread_local! {
    static SCHEDULER: Box<RefCell<*mut Scheduler>> = Box::new(RefCell::new(std::ptr::null_mut()));
    static YIELDER: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
    static TIMEOUT_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
    static RESULTS: Box<RefCell<*mut ObjectMap<usize>>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

/// 主协程
type MainCoroutine<'a> = OpenCoroutine<'a, (), (), ()>;

static mut SYSTEM_CALL_TABLE: Lazy<ObjectMap<usize>> = Lazy::new(ObjectMap::new);

static mut SUSPEND_TABLE: Lazy<TimerList> = Lazy::new(TimerList::new);

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler {
    id: usize,
    ready: &'static mut WorkStealQueue,
    //not support for now
    copy_stack: ObjectList,
}

impl Scheduler {
    pub fn new() -> Self {
        #[cfg(unix)]
        unsafe {
            extern "C" fn sigurg_handler(_signal: libc::c_int) {
                //挂起当前协程
                let yielder: *const Yielder<&'static mut c_void, (), &'static mut c_void> =
                    OpenCoroutine::yielder();
                if !yielder.is_null() {
                    unsafe {
                        (*yielder).suspend(());
                    }
                }
            }
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = sigurg_handler as libc::sighandler_t;
            libc::sigaddset(&mut act.sa_mask, libc::SIGURG);
            act.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGURG, &act, std::ptr::null_mut());
        }
        Scheduler {
            id: IdGenerator::next_scheduler_id(),
            ready: get_queue(),
            copy_stack: ObjectList::new(),
        }
    }

    fn init_current(scheduler: &mut Scheduler) {
        SCHEDULER.with(|boxed| {
            *boxed.borrow_mut() = scheduler;
        });
    }

    pub fn current<'a>() -> Option<&'a mut Scheduler> {
        SCHEDULER.with(|boxed| unsafe {
            let mut ptr = boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(&mut *(*ptr))
            }
        })
    }

    fn init_yielder(yielder: &Yielder<(), (), ()>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *const _ as *const c_void;
        });
    }

    fn yielder<'a>() -> *const Yielder<'a, (), (), ()> {
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

    fn init_results(result: &mut ObjectMap<usize>) {
        RESULTS.with(|boxed| {
            *boxed.borrow_mut() = result;
        });
    }

    fn results() -> *mut ObjectMap<usize> {
        RESULTS.with(|boxed| *boxed.borrow_mut())
    }

    fn clean_results() {
        RESULTS.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut())
    }

    pub fn submit(
        &mut self,
        f: UserFunc<&'static mut c_void, (), &'static mut c_void>,
        val: &'static mut c_void,
        size: usize,
    ) -> Result<(), StackError> {
        let mut coroutine = Coroutine::new(f, val, size)?;
        coroutine.status = Status::Ready;
        self.ready.push_back(coroutine);
        Ok(())
    }

    pub fn timed_schedule(&mut self, timeout: Duration) -> Result<ObjectMap<usize>, StackError> {
        let timeout_time = timer_utils::get_timeout_time(timeout);
        let mut scheduled = ObjectMap::new();
        while !self.ready.is_empty()
            || unsafe { !SUSPEND_TABLE.is_empty() || !SYSTEM_CALL_TABLE.is_empty() }
            || !self.copy_stack.is_empty()
        {
            if timeout_time <= timer_utils::now() {
                break;
            }
            let temp = self.try_timeout_schedule(timeout_time)?;
            for (k, v) in temp {
                scheduled.insert(k, v);
            }
        }
        Ok(scheduled)
    }

    pub fn try_schedule(&mut self) -> Result<ObjectMap<usize>, StackError> {
        self.try_timeout_schedule(Duration::MAX.as_secs())
    }

    pub fn try_timed_schedule(&mut self, time: Duration) -> Result<ObjectMap<usize>, StackError> {
        self.try_timeout_schedule(timer_utils::get_timeout_time(time))
    }

    pub fn try_timeout_schedule(
        &mut self,
        timeout_time: u64,
    ) -> Result<ObjectMap<usize>, StackError> {
        let mut result = ObjectMap::new();
        Scheduler::init_current(self);
        Scheduler::init_results(&mut result);
        Scheduler::init_timeout_time(timeout_time);
        #[allow(improper_ctypes_definitions)]
        extern "C" fn main_context_func(yielder: &Yielder<(), (), ()>, _param: ()) {
            Scheduler::init_yielder(yielder);
            Scheduler::current()
                .expect("current scheduler not inited !")
                .do_schedule();
            unreachable!("should not execute to here !")
        }
        let mut main = MainCoroutine::new(main_context_func, (), Stack::default_size())?;
        main.resume();
        Scheduler::clean_results();
        Scheduler::clean_time();
        Ok(result)
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
        self.check_ready();
        match self.ready.pop_front_raw() {
            Some(pointer) => {
                let coroutine = unsafe {
                    &mut *(pointer as *mut Coroutine<&'static mut c_void, &'static mut c_void>)
                };
                let _start = timer_utils::get_timeout_time(Duration::from_millis(10));
                #[cfg(unix)]
                {
                    Monitor::init_signal_time(_start);
                    Monitor::add_task(_start);
                }
                match coroutine.resume() {
                    CoroutineResult::Yield(()) => {
                        let delay_time =
                            Yielder::<&'static mut c_void, (), &'static mut c_void>::delay_time();
                        if delay_time > 0 {
                            //挂起协程到时间轮
                            coroutine.status = Status::Suspend;
                            unsafe {
                                SUSPEND_TABLE.insert_raw(
                                    timer_utils::add_timeout_time(delay_time),
                                    coroutine as *mut _ as *mut c_void,
                                );
                            }
                            Yielder::<&'static mut c_void, (), &'static mut c_void>::clean_delay();
                        } else {
                            //直接切换到下一个协程执行
                            self.ready.push_back_raw(coroutine as *mut _ as *mut c_void);
                        }
                    }
                    CoroutineResult::Return(_) => unreachable!("never have a result"),
                };
                #[cfg(unix)]
                {
                    //还没执行到10ms就主动yield了，此时需要清理signal
                    //否则下一个协程执行不到10ms就被抢占调度了
                    Monitor::clean_task(_start);
                    Monitor::clean_signal_time();
                }
                self.do_schedule();
            }
            None => Scheduler::back_to_main(),
        }
    }

    fn check_ready(&mut self) {
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
                            if let Some(pointer) = entry.pop_front_raw() {
                                let coroutine = &mut *(pointer
                                    as *mut Coroutine<&'static mut c_void, &'static mut c_void>);
                                coroutine.status = Status::Ready;
                                //优先执行到时间的协程
                                self.ready.push_back_raw(coroutine as *mut _ as *mut c_void);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 用户不应该使用此方法
    pub fn syscall(&mut self) {
        if let Some(co) = Coroutine::<&'static mut c_void, &'static mut c_void>::current() {
            unsafe { SYSTEM_CALL_TABLE.insert(co.get_id(), co) };
            Coroutine::<&'static mut c_void, &'static mut c_void>::clean_current();
        }
    }

    /// 用户不应该使用此方法
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn resume(&mut self, co_id: usize) {
        if let Some(co) = SYSTEM_CALL_TABLE.remove(&co_id) {
            self.ready.push_back_raw(co)
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::coroutine::Yielder;
    use crate::scheduler::Scheduler;
    use std::os::raw::c_void;
    use std::thread;
    use std::time::Duration;

    fn null() -> &'static mut c_void {
        unsafe { std::mem::transmute(10usize) }
    }

    #[test]
    fn simplest() {
        let mut scheduler = Scheduler::new();
        extern "C" fn f1(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine1] launched");
            null()
        }
        scheduler.submit(f1, null(), 4096).expect("submit failed !");
        extern "C" fn f2(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine2] launched");
            null()
        }
        scheduler.submit(f2, null(), 4096).expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    #[test]
    fn with_suspend() {
        let mut scheduler = Scheduler::new();
        extern "C" fn suspend1(
            yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine1] suspend");
            yielder.suspend(());
            println!("[coroutine1] back");
            null()
        }
        scheduler
            .submit(suspend1, null(), 4096)
            .expect("submit failed !");
        extern "C" fn suspend2(
            yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine2] suspend");
            yielder.suspend(());
            println!("[coroutine2] back");
            null()
        }
        scheduler
            .submit(suspend2, null(), 4096)
            .expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    extern "C" fn delay(
        yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
        _input: &'static mut c_void,
    ) -> &'static mut c_void {
        println!("[coroutine] delay");
        yielder.delay((), 100);
        println!("[coroutine] back");
        null()
    }

    #[test]
    fn with_delay() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(delay, null(), 4096)
            .expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
        thread::sleep(Duration::from_millis(100));
        scheduler.try_schedule().expect("try_schedule failed !");
    }

    #[test]
    fn timed_schedule() {
        let mut scheduler = Scheduler::new();
        scheduler
            .submit(delay, null(), 4096)
            .expect("submit failed !");
        scheduler
            .timed_schedule(Duration::from_millis(200))
            .expect("try_schedule failed !");
    }

    #[cfg(unix)]
    #[test]
    fn preemptive_schedule() {
        static mut FLAG: bool = true;
        let handler = std::thread::spawn(|| {
            let mut scheduler = Scheduler::new();
            extern "C" fn f1(
                _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
                _input: &'static mut c_void,
            ) -> &'static mut c_void {
                unsafe {
                    while FLAG {
                        println!("loop");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                null()
            }
            scheduler.submit(f1, null(), 4096).expect("submit failed !");
            extern "C" fn f2(
                _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
                _input: &'static mut c_void,
            ) -> &'static mut c_void {
                unsafe {
                    FLAG = false;
                }
                null()
            }
            scheduler.submit(f2, null(), 4096).expect("submit failed !");
            scheduler.try_schedule().expect("try_schedule failed !");
        });
        unsafe {
            handler.join().unwrap();
            assert!(!FLAG);
        }
    }
}
