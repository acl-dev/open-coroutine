use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine, Yielder};
use id_generator::IdGenerator;
use object_list::ObjectList;
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::collections::HashMap;
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
type MainCoroutine<'a, Yield> = OpenCoroutine<'a, (), Yield, ()>;

/// 子协程
pub type Coroutine<Input, Return> = OpenCoroutine<'static, Input, (), Return>;

#[repr(C)]
pub struct OpenCoroutine<'a, Input, Yield, Return> {
    //协程状id
    id: usize,
    //协程状态
    status: Status,
    inner: Option<ScopedCoroutine<'a, Input, Yield, Return, DefaultStack>>,
    //调用用户函数的参数
    param: ManuallyDrop<Input>,
    //最近一次yield的参数
    //last_yield: Yield,
}

impl<'a, Input, Return> OpenCoroutine<'a, Input, (), Return> {
    pub fn new<F>(f: F, val: Input, size: usize) -> Self
    where
        F: FnOnce(&Yielder<Input, ()>, Input) -> Return,
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
                let _result = f(yielder, input);
                //todo 实现个ObjectMap来保存结果
                //RESULTS.insert(coroutine.id, _result);
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

static mut RESULTS: Lazy<HashMap<usize, Option<*mut c_void>>> = Lazy::new(HashMap::new);

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
        F: FnOnce(&Yielder<Option<*mut c_void>, ()>, Option<*mut c_void>) -> Option<*mut c_void>,
        F: 'static,
    {
        let mut coroutine = Coroutine::new(f, val, size);
        coroutine.status = Status::Ready;
        self.ready.push_back(coroutine);
    }

    pub fn try_schedule(&mut self) -> &HashMap<usize, Option<*mut c_void>> {
        self.try_timed_schedule(Duration::MAX)
    }

    pub fn try_timed_schedule(
        &mut self,
        timeout: Duration,
    ) -> &HashMap<usize, Option<*mut c_void>> {
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
        unsafe { &RESULTS }
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
        match self.ready.pop_front_raw() {
            Some(pointer) => {
                let mut coroutine = unsafe {
                    std::ptr::read_unaligned(
                        pointer as *mut Coroutine<Option<*mut c_void>, Option<*mut c_void>>,
                    )
                };
                self.running = Some(coroutine.id);
                match coroutine.resume() {
                    CoroutineResult::Yield(()) => {
                        //切换到下一个协程执行
                        self.ready.push_back(coroutine);
                    }
                    CoroutineResult::Return(_) => unreachable!("never have a result"),
                };
                self.running = None;
                self.do_schedule();
            }
            None => Scheduler::back_to_main(),
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

    #[test]
    fn test() {
        println!("[main] creating coroutine");

        let mut main_coroutine = corosensei::Coroutine::new(|main_yielder, input| {
            println!("[main coroutine] launched");
            let main_yielder =
                unsafe { std::ptr::read_unaligned(main_yielder as *const Yielder<(), i32>) };

            let mut coroutine2 = corosensei::Coroutine::new(move |_: &Yielder<(), ()>, input| {
                println!("[coroutine2] launched");
                main_yielder.suspend(1);
                2
            });

            let mut coroutine1 = corosensei::Coroutine::new(move |_: &Yielder<(), ()>, input| {
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
        let mut scheduler = Scheduler::current();
        scheduler.submit(
            move |yielder, input| {
                println!("[coroutine1] launched");
                None
            },
            Some(1 as *mut c_void),
            4096,
        );
        scheduler.submit(
            move |yielder, input| {
                println!("[coroutine2] launched");
                Some(1 as *mut c_void)
            },
            Some(3 as *mut c_void),
            4096,
        );
        scheduler.try_schedule();
    }
}
