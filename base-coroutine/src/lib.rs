use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine, Yielder};
use id_generator::IdGenerator;
use object_collection::{ObjectList, ObjectMap};
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::os::raw::c_void;
use std::time::Duration;
use timer::TimerList;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Status {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起
    Suspend,
    ///执行系统调用
    SystemCall,
    ///栈扩/缩容时
    CopyStack,
    ///调用用户函数完成，但未退出
    Finished,
    ///已退出
    Exited,
}

thread_local! {
    static DELAY_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

fn init_delay_time(time: u64) {
    DELAY_TIME.with(|boxed| {
        *boxed.borrow_mut() = time;
    });
}

fn delay_time() -> u64 {
    DELAY_TIME.with(|boxed| *boxed.borrow_mut())
}

fn clean_delay() {
    DELAY_TIME.with(|boxed| *boxed.borrow_mut() = 0)
}

#[repr(transparent)]
pub struct OpenYielder<'a, Input>(&'a Yielder<Input, ()>);

impl<'a, Input> OpenYielder<'a, Input> {
    pub extern "C" fn suspend(&self) -> Input {
        self.0.suspend(())
    }

    pub extern "C" fn delay(&self, ms_time: u64) -> Input {
        self.delay_ns(ms_time * 1_000_000)
    }

    pub extern "C" fn delay_ns(&self, ns_time: u64) -> Input {
        init_delay_time(ns_time);
        self.suspend()
    }
}

/**
主线程 -> 主协程(取得子协程的所有权,即scheduler)
           ↓
         子协程1
           ↓
主线程 •• 子协程2(超时提前返回)
           ↓
         ......
           ↓
主线程 <- 子协程N
 */

/// 主线程
type MainCoroutine<'a> = OpenCoroutine<'a, (), (), ()>;

/// 子协程
pub type Coroutine<Input, Return> = OpenCoroutine<'static, Input, (), Return>;

thread_local! {
    static COROUTINE: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

#[repr(C)]
pub struct OpenCoroutine<'a, Input, Yield, Return> {
    //协程状id
    id: usize,
    //协程状态
    status: Status,
    inner: Option<ScopedCoroutine<'a, Input, Yield, Return, DefaultStack>>,
    //调用用户函数的参数
    param: ManuallyDrop<Input>,
}

pub type ContextFn<Input, Return> = extern "C" fn(&OpenYielder<Input>, Input) -> Return;

impl<'a, Input, Return> OpenCoroutine<'a, Input, (), Return> {
    pub fn new(f: ContextFn<Input, Return>, val: Input, size: usize) -> Self {
        let mut coroutine = OpenCoroutine {
            id: IdGenerator::next_coroutine_id(),
            status: Status::Created,
            inner: None,
            param: ManuallyDrop::new(val),
        };
        coroutine.inner = Some(ScopedCoroutine::with_stack(
            DefaultStack::new(size).expect("failed to allocate stack"),
            move |yielder, input| {
                let current: *mut OpenCoroutine<'_, Input, (), Return> = OpenCoroutine::current();
                unsafe {
                    (*current).status = Status::Running;
                    let result = f(&OpenYielder(yielder), input);
                    (*current).status = Status::Finished;
                    let results = Scheduler::results();
                    (*results).insert((*current).id, result);
                }
                OpenCoroutine::<Input, (), Return>::clean();
                Scheduler::current().do_schedule();
                unreachable!("should not execute to here !")
            },
        ));
        coroutine
    }

    pub fn resume(&mut self) -> CoroutineResult<(), Return> {
        unsafe {
            self.inner
                .as_mut()
                .unwrap()
                .resume(ManuallyDrop::take(&mut self.param))
        }
    }

    pub fn resume_with(&mut self, val: Input) -> CoroutineResult<(), Return> {
        self.inner.as_mut().unwrap().resume(val)
    }

    fn init(coroutine: &mut OpenCoroutine<Input, (), Return>) {
        COROUTINE.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *mut _ as *mut c_void;
        });
    }

    fn current() -> *mut OpenCoroutine<'a, Input, (), Return> {
        COROUTINE.with(|boxed| *boxed.borrow_mut() as *mut OpenCoroutine<Input, (), Return>)
    }

    fn clean() {
        COROUTINE.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut())
    }
}

impl<'a, Yield> OpenCoroutine<'a, (), Yield, ()> {
    fn create<F>(f: F, size: usize) -> Self
    where
        F: FnOnce(&Yielder<(), Yield>, ()),
        F: 'a,
    {
        OpenCoroutine {
            id: IdGenerator::next_coroutine_id(),
            status: Status::Created,
            inner: Some(ScopedCoroutine::with_stack(
                DefaultStack::new(size).expect("failed to allocate stack"),
                f,
            )),
            param: ManuallyDrop::new(()),
        }
    }

    fn start(&mut self) -> Option<Yield> {
        self.inner.as_mut().unwrap().resume(()).as_yield()
    }
}

thread_local! {
    static SCHEDULER: Box<Scheduler> = Box::new(Scheduler::new());
    static YIELDER: Box<RefCell<*const Yielder<(), ()>>> = Box::new(RefCell::new(std::ptr::null()));
    static TIMEOUT_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
    static RESULTS: Box<RefCell<*mut ObjectMap<usize>>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler {
    id: usize,
    ready: ObjectList,
    //正在执行的协程id
    running: Option<usize>,
    suspend: TimerList,
    //not support for now
    #[allow(unused)]
    system_call: ObjectList,
    //not support for now
    #[allow(unused)]
    copy_stack: ObjectList,
    finished: ObjectList,
}

impl Scheduler {
    fn new() -> Self {
        Scheduler {
            id: IdGenerator::next_scheduler_id(),
            ready: ObjectList::new(),
            running: None,
            suspend: TimerList::new(),
            system_call: ObjectList::new(),
            copy_stack: ObjectList::new(),
            finished: ObjectList::new(),
        }
    }

    pub fn current<'a>() -> &'a mut Scheduler {
        SCHEDULER.with(|boxed| Box::leak(unsafe { std::ptr::read_unaligned(boxed) }))
    }

    fn init_yielder(yielder: &Yielder<(), ()>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder;
        });
    }

    fn yielder() -> *const Yielder<(), ()> {
        YIELDER.with(|boxed| *boxed.borrow_mut())
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
        f: ContextFn<Option<&'static mut c_void>, Option<&'static mut c_void>>,
        val: Option<&'static mut c_void>,
        size: usize,
    ) {
        let mut coroutine = Coroutine::new(f, val, size);
        coroutine.status = Status::Ready;
        self.ready.push_back(coroutine);
    }

    pub fn try_schedule(&mut self) -> ObjectMap<usize> {
        self.try_timed_schedule(Duration::MAX)
    }

    pub fn try_timed_schedule(&mut self, timeout: Duration) -> ObjectMap<usize> {
        let mut result = ObjectMap::new();
        Scheduler::init_results(&mut result);
        let mut main = MainCoroutine::create(
            |main_yielder, _input| {
                let timeout_time = timer::get_timeout_time(timeout);
                Scheduler::init_timeout_time(timeout_time);
                Scheduler::init_yielder(main_yielder);
                self.do_schedule();
                unreachable!("should not execute to here !")
            },
            128 * 1024,
        );
        main.start();
        Scheduler::clean_results();
        result
    }

    fn back_to_main() {
        //跳回主线程
        let yielder = Scheduler::yielder();
        Scheduler::clean_yielder();
        Scheduler::clean_time();
        if !yielder.is_null() {
            unsafe {
                (*yielder).suspend(());
            }
        }
    }

    fn do_schedule(&mut self) {
        if Scheduler::timeout_time() <= timer::now() {
            Scheduler::back_to_main()
        }
        self.check_ready();
        match self.ready.pop_front_raw() {
            Some(pointer) => {
                let mut coroutine = unsafe {
                    std::ptr::read_unaligned(
                        pointer as *mut Coroutine<Option<*mut c_void>, Option<*mut c_void>>,
                    )
                };
                self.running = Some(coroutine.id);
                OpenCoroutine::init(&mut coroutine);
                match coroutine.resume() {
                    CoroutineResult::Yield(()) => {
                        let delay_time = delay_time();
                        if delay_time > 0 {
                            //挂起协程到时间轮
                            coroutine.status = Status::Suspend;
                            self.suspend
                                .insert(timer::add_timeout_time(delay_time), coroutine);
                            clean_delay();
                        } else {
                            //直接切换到下一个协程执行
                            self.ready.push_back(coroutine);
                        }
                    }
                    CoroutineResult::Return(_) => unreachable!("never have a result"),
                };
                self.running = None;
                self.do_schedule();
            }
            None => Scheduler::back_to_main(),
        }
    }

    fn check_ready(&mut self) {
        for _ in 0..self.suspend.len() {
            if let Some(entry) = self.suspend.front() {
                let exec_time = entry.get_time();
                if timer::now() < exec_time {
                    break;
                }
                //移动至"就绪"队列
                if let Some(mut entry) = self.suspend.pop_front() {
                    for _ in 0..entry.len() {
                        if let Some(pointer) = entry.pop_front_raw() {
                            let mut coroutine = unsafe {
                                std::ptr::read_unaligned(
                                    pointer
                                        as *mut Coroutine<Option<*mut c_void>, Option<*mut c_void>>,
                                )
                            };
                            coroutine.status = Status::Ready;
                            //优先执行到时间的协程
                            self.ready.push_front(coroutine);
                        }
                    }
                }
            }
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
    use crate::{OpenYielder, Scheduler};
    use corosensei::{CoroutineResult, Yielder};
    use std::os::raw::c_void;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test() {
        println!("[main] creating coroutine");

        let mut main_coroutine = corosensei::Coroutine::new(|main_yielder, _input| {
            println!("[main coroutine] launched");
            let main_yielder =
                unsafe { std::ptr::read_unaligned(main_yielder as *const Yielder<(), i32>) };

            let mut coroutine2 = corosensei::Coroutine::new(move |_: &Yielder<(), ()>, _input| {
                println!("[coroutine2] launched");
                main_yielder.suspend(1);
                2
            });

            let mut coroutine1 = corosensei::Coroutine::new(move |_: &Yielder<(), ()>, _input| {
                println!("[coroutine1] launched");
                //这里loop + match确保子协程coroutine2不被中断
                coroutine2.resume(());
            });
            //这里loop + match确保子协程coroutine1不被中断
            coroutine1.resume(());
            3
        });

        println!("[main] resuming coroutine");
        match main_coroutine.resume(()) {
            CoroutineResult::Yield(i) => println!("[main] got {:?} from coroutine", i),
            CoroutineResult::Return(r) => {
                println!("[main] got result {:?} from coroutine", r);
            }
        }

        println!("[main] exiting");
    }

    #[test]
    fn simplest() {
        let scheduler = Scheduler::current();
        extern "C" fn f1(
            _yielder: &OpenYielder<Option<&'static mut c_void>>,
            _input: Option<&'static mut c_void>,
        ) -> Option<&'static mut c_void> {
            println!("[coroutine1] launched");
            None
        }
        scheduler.submit(f1, None, 4096);
        extern "C" fn f2(
            _yielder: &OpenYielder<Option<&'static mut c_void>>,
            _input: Option<&'static mut c_void>,
        ) -> Option<&'static mut c_void> {
            println!("[coroutine2] launched");
            None
        }
        scheduler.submit(f2, None, 4096);
        scheduler.try_schedule();
    }

    #[test]
    fn with_suspend() {
        let scheduler = Scheduler::current();
        extern "C" fn suspend1(
            yielder: &OpenYielder<Option<&'static mut c_void>>,
            _input: Option<&'static mut c_void>,
        ) -> Option<&'static mut c_void> {
            println!("[coroutine1] suspend");
            yielder.suspend();
            println!("[coroutine1] back");
            None
        }
        scheduler.submit(suspend1, None, 4096);
        extern "C" fn suspend2(
            yielder: &OpenYielder<Option<&'static mut c_void>>,
            _input: Option<&'static mut c_void>,
        ) -> Option<&'static mut c_void> {
            println!("[coroutine2] suspend");
            yielder.suspend();
            println!("[coroutine2] back");
            None
        }
        scheduler.submit(suspend2, None, 4096);
        scheduler.try_schedule();
    }

    #[test]
    fn with_delay() {
        let scheduler = Scheduler::current();
        extern "C" fn delay(
            yielder: &OpenYielder<Option<&'static mut c_void>>,
            _input: Option<&'static mut c_void>,
        ) -> Option<&'static mut c_void> {
            println!("[coroutine] delay");
            yielder.delay(100);
            println!("[coroutine] back");
            None
        }
        scheduler.submit(delay, None, 4096);
        scheduler.try_schedule();
        thread::sleep(Duration::from_millis(100));
        scheduler.try_schedule();
    }
}
