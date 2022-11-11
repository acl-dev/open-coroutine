use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine, Yielder};
use id_generator::IdGenerator;
use object_collection::ObjectList;
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
    static DELAY_TIME: Box<RefCell<Duration>> = Box::new(RefCell::new(Duration::from_nanos(0)));
}

fn init_delay_time(time: Duration) {
    DELAY_TIME.with(|boxed| {
        *boxed.borrow_mut() = time;
    });
}

fn delay_time() -> Duration {
    DELAY_TIME.with(|boxed| *boxed.borrow_mut())
}

fn clean_delay() {
    DELAY_TIME.with(|boxed| *boxed.borrow_mut() = Duration::from_nanos(0))
}

pub struct OpenYielder<'a, Input>(&'a Yielder<Input, ()>);

impl<'a, Input> OpenYielder<'a, Input> {
    pub fn suspend(&self) -> Input {
        self.0.suspend(())
    }

    pub fn delay(&self, time: Duration) -> Input {
        init_delay_time(time);
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

impl<'a, Input, Return> OpenCoroutine<'a, Input, (), Return> {
    pub fn new<F>(f: F, val: Input, size: usize) -> Self
    where
        F: FnOnce(&OpenYielder<Input>, Input) -> Return,
        F: 'a,
    {
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
                }
                let _result = f(&OpenYielder(yielder), input);
                unsafe {
                    (*current).status = Status::Finished;
                }
                OpenCoroutine::<Input, (), Return>::clean();
                //todo 实现个ObjectMap来保存结果
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

    pub fn submit<F>(&mut self, f: F, val: Option<*mut c_void>, size: usize)
    where
        F: FnOnce(&OpenYielder<Option<*mut c_void>>, Option<*mut c_void>) -> Option<*mut c_void>,
        F: 'static,
    {
        let mut coroutine = Coroutine::new(f, val, size);
        coroutine.status = Status::Ready;
        self.ready.push_back(coroutine);
    }

    pub fn try_schedule(&mut self) {
        self.try_timed_schedule(Duration::MAX)
    }

    pub fn try_timed_schedule(
        &mut self,
        timeout: Duration,
    ) {
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
                        let time = timer::dur_to_ns(delay_time);
                        if time > 0 {
                            //挂起协程到时间轮
                            coroutine.status = Status::Suspend;
                            self.suspend
                                .insert(timer::get_timeout_time(delay_time), coroutine);
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
    use crate::Scheduler;
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
        scheduler.submit(
            move |_yielder, _input| {
                println!("[coroutine1] launched");
                None
            },
            Some(1 as *mut c_void),
            4096,
        );
        scheduler.submit(
            move |_yielder, _input| {
                println!("[coroutine2] launched");
                Some(1 as *mut c_void)
            },
            Some(3 as *mut c_void),
            4096,
        );
        scheduler.try_schedule();
    }

    #[test]
    fn with_suspend() {
        let scheduler = Scheduler::current();
        scheduler.submit(
            move |yielder, _input| {
                println!("[coroutine1] suspend");
                yielder.suspend();
                println!("[coroutine1] back");
                None
            },
            Some(1 as *mut c_void),
            4096,
        );
        scheduler.submit(
            move |yielder, _input| {
                println!("[coroutine2] suspend");
                yielder.suspend();
                println!("[coroutine2] back");
                Some(1 as *mut c_void)
            },
            Some(3 as *mut c_void),
            4096,
        );
        scheduler.try_schedule();
    }

    #[test]
    fn with_delay() {
        let scheduler = Scheduler::current();
        scheduler.submit(
            move |yielder, _input| {
                println!("[coroutine] delay");
                yielder.delay(Duration::from_millis(100));
                println!("[coroutine] back");
                None
            },
            None,
            4096,
        );
        scheduler.try_schedule();
        thread::sleep(Duration::from_millis(100));
        scheduler.try_schedule();
    }
}
