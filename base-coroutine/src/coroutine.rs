use crate::context::{Context, Transfer};
use crate::id::IdGenerator;
#[cfg(unix)]
use crate::monitor::Monitor;
use crate::scheduler::Scheduler;
use crate::stack::ProtectedFixedSizeStack;
use crate::stack::StackError::{ExceedsMaximumSize, IoError};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::os::raw::c_void;

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

#[repr(transparent)]
pub struct Yielder<'a, Param, Yield, Return> {
    sp: &'a Transfer,
    marker: PhantomData<fn(Yield) -> CoroutineResult<Param, Return>>,
}

thread_local! {
    static DELAY_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
}

impl<'a, Param, Yield, Return> Yielder<'a, Param, Yield, Return> {
    /// Suspends the execution of a currently running coroutine.
    ///
    /// This function will switch control back to the original caller of
    /// [`Coroutine::resume`]. This function will then return once the
    /// [`Coroutine::resume`] function is called again.
    pub extern "C" fn suspend(&self, val: Yield) -> Param {
        OpenCoroutine::<Param, Yield, Return>::clean_current();
        let yielder = OpenCoroutine::<Param, Yield, Return>::yielder();
        OpenCoroutine::<Param, Yield, Return>::clean_yielder();
        unsafe {
            let mut coroutine_result = CoroutineResult::<Yield, Return>::Yield(val);
            //see Scheduler.do_schedule
            let transfer = self
                .sp
                .context
                .resume(&mut coroutine_result as *mut _ as usize);
            OpenCoroutine::init_yielder(&*yielder);
            let backed = transfer.data as *mut c_void as *mut _
                as *mut OpenCoroutine<'_, Param, Yield, Return>;
            std::ptr::read_unaligned(&(*backed).param)
        }
    }

    pub(crate) extern "C" fn syscall(&self) {
        OpenCoroutine::<Param, Yield, Return>::clean_current();
        let yielder = OpenCoroutine::<Param, Yield, Return>::yielder();
        OpenCoroutine::<Param, Yield, Return>::clean_yielder();
        unsafe {
            let mut coroutine_result = CoroutineResult::<Yield, Return>::SystemCall;
            //see Scheduler.do_schedule
            self.sp
                .context
                .resume(&mut coroutine_result as *mut _ as usize);
            OpenCoroutine::init_yielder(&*yielder);
        }
    }

    pub extern "C" fn delay(&self, val: Yield, ms_time: u64) -> Param {
        self.delay_ns(
            val,
            match ms_time.checked_mul(1_000_000) {
                Some(v) => v,
                None => u64::MAX,
            },
        )
    }

    pub extern "C" fn delay_ns(&self, val: Yield, ns_time: u64) -> Param {
        Yielder::<Param, Yield, Return>::init_delay_time(ns_time);
        self.suspend(val)
    }

    fn init_delay_time(time: u64) {
        DELAY_TIME.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn delay_time() -> u64 {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_delay() {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }
}

/// Value returned from resuming a coroutine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoroutineResult<Yield, Return> {
    /// Value returned by a coroutine suspending itself with a `Yielder`.
    Yield(Yield),

    /// Value returned by a coroutine returning from its main function.
    Return(Return),

    SystemCall,
}

impl<Yield, Return> CoroutineResult<Yield, Return> {
    /// Returns the `Yield` value as an `Option<Yield>`.
    pub fn as_yield(self) -> Option<Yield> {
        match self {
            CoroutineResult::Yield(val) => Some(val),
            CoroutineResult::Return(_) => None,
            CoroutineResult::SystemCall => None,
        }
    }

    /// Returns the `Return` value as an `Option<Return>`.
    pub fn as_return(self) -> Option<Return> {
        match self {
            CoroutineResult::Yield(_) => None,
            CoroutineResult::Return(val) => Some(val),
            CoroutineResult::SystemCall => None,
        }
    }
}

pub type UserFunc<'a, Param, Yield, Return> =
    extern "C" fn(&'a Yielder<Param, Yield, Return>, Param) -> Return;

/// 子协程
pub type Coroutine<Input, Return> = OpenCoroutine<'static, Input, (), Return>;

thread_local! {
    static COROUTINE: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
    static YIELDER: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

#[repr(C)]
pub struct OpenCoroutine<'a, Param, Yield, Return> {
    id: usize,
    sp: Transfer,
    stack: ProtectedFixedSizeStack,
    pub(crate) status: Status,
    //用户函数
    proc: UserFunc<'a, Param, Yield, Return>,
    marker: PhantomData<&'a extern "C" fn(Param) -> CoroutineResult<Yield, Return>>,
    //调用用户函数的参数
    param: Param,
}

impl<'a, Param, Yield, Return> OpenCoroutine<'a, Param, Yield, Return> {
    extern "C" fn child_context_func(t: Transfer) {
        let coroutine =
            t.data as *mut c_void as *mut _ as *mut OpenCoroutine<'_, Param, Yield, Return>;
        let yielder = Yielder {
            sp: &t,
            marker: Default::default(),
        };
        OpenCoroutine::init_yielder(&yielder);
        unsafe {
            (*coroutine).status = Status::Running;
            let proc = (*coroutine).proc;
            let param = std::ptr::read_unaligned(&(*coroutine).param);
            let result = proc(&yielder, param);
            (*coroutine).status = Status::Finished;
            OpenCoroutine::<Param, Yield, Return>::clean_current();
            OpenCoroutine::<Param, Yield, Return>::clean_yielder();
            #[cfg(unix)]
            {
                //还没执行到10ms就返回了，此时需要清理signal
                //否则下一个协程执行不到10ms就被抢占调度了
                Monitor::clean_task(Monitor::signal_time());
                Monitor::clean_signal_time();
            }
            if let Some(scheduler) = Scheduler::current() {
                //执行下一个子协程
                scheduler.do_schedule();
            }
            let mut coroutine_result = CoroutineResult::<Yield, Return>::Return(result);
            t.context.resume(&mut coroutine_result as *mut _ as usize);
            unreachable!("should not execute to here !")
        }
    }

    pub fn new(
        proc: UserFunc<'a, Param, Yield, Return>,
        param: Param,
        size: usize,
    ) -> std::io::Result<Self> {
        let stack = ProtectedFixedSizeStack::new(size).map_err(|e| match e {
            ExceedsMaximumSize(size) => std::io::Error::new(
                std::io::ErrorKind::Other,
                "Requested more than max size of ".to_owned()
                    + &size.to_string()
                    + " bytes for a stack",
            ),
            IoError(e) => e,
        })?;
        Ok(OpenCoroutine {
            id: IdGenerator::next_coroutine_id(),
            sp: Transfer::new(
                unsafe {
                    Context::new(
                        &stack,
                        OpenCoroutine::<Param, Yield, Return>::child_context_func,
                    )
                },
                0,
            ),
            stack,
            status: Status::Created,
            proc,
            marker: Default::default(),
            param,
        })
    }

    pub fn resume_with(&mut self, val: Param) -> CoroutineResult<Yield, Return> {
        self.param = val;
        self.resume()
    }

    pub fn resume(&mut self) -> CoroutineResult<Yield, Return> {
        self.status = Status::Ready;
        self.sp.data = self as *mut _ as usize;
        unsafe {
            OpenCoroutine::init_current(self);
            let transfer = self.sp.context.resume(self.sp.data);
            //更新sp
            self.sp.context = transfer.context;
            std::ptr::read_unaligned(
                transfer.data as *mut c_void as *mut _ as *mut CoroutineResult<Yield, Return>,
            )
        }
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn get_status(&self) -> Status {
        self.status
    }

    fn init_yielder(yielder: &Yielder<Param, Yield, Return>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *const _ as *const c_void;
        });
    }

    pub fn yielder<'y>() -> *const Yielder<'y, Param, Yield, Return> {
        YIELDER.with(|boxed| unsafe { std::mem::transmute(*boxed.borrow_mut()) })
    }

    fn clean_yielder() {
        YIELDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null())
    }

    fn init_current(coroutine: &mut OpenCoroutine<'a, Param, Yield, Return>) {
        COROUTINE.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *mut _ as *mut c_void;
        })
    }

    pub fn current<'c>() -> Option<&'a mut OpenCoroutine<'c, Param, Yield, Return>> {
        COROUTINE.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &mut *(ptr as *mut OpenCoroutine<Param, Yield, Return>) })
            }
        })
    }

    fn clean_current() {
        COROUTINE.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut())
    }
}

impl<'a, Param, Yield, Return> Drop for OpenCoroutine<'a, Param, Yield, Return> {
    fn drop(&mut self) {
        self.status = Status::Exited;
    }
}

#[cfg(test)]
mod tests {
    use crate::coroutine::{OpenCoroutine, Yielder};

    #[test]
    fn test_return() {
        extern "C" fn context_func(_yielder: &Yielder<usize, usize, usize>, input: usize) -> usize {
            assert_eq!(0, input);
            1
        }
        let mut coroutine =
            OpenCoroutine::new(context_func, 0, 2048).expect("create coroutine failed !");
        assert_eq!(1, coroutine.resume_with(0).as_return().unwrap());
    }

    #[test]
    fn test_yield_once() {
        extern "C" fn context_func(yielder: &Yielder<usize, usize, usize>, input: usize) -> usize {
            assert_eq!(1, input);
            assert_eq!(3, yielder.suspend(2));
            6
        }
        let mut coroutine =
            OpenCoroutine::new(context_func, 1, 2048).expect("create coroutine failed !");
        assert_eq!(2, coroutine.resume_with(1).as_yield().unwrap());
    }

    #[test]
    fn test_yield() {
        extern "C" fn context_func(yielder: &Yielder<usize, usize, usize>, input: usize) -> usize {
            assert_eq!(1, input);
            assert_eq!(3, yielder.suspend(2));
            assert_eq!(5, yielder.suspend(4));
            6
        }
        let mut coroutine =
            OpenCoroutine::new(context_func, 1, 2048).expect("create coroutine failed !");
        assert_eq!(2, coroutine.resume_with(1).as_yield().unwrap());
        assert_eq!(4, coroutine.resume_with(3).as_yield().unwrap());
        assert_eq!(6, coroutine.resume_with(5).as_return().unwrap());
    }

    #[test]
    fn test_current() {
        extern "C" fn context_func(
            _yielder: &Yielder<usize, usize, usize>,
            _input: usize,
        ) -> usize {
            assert!(OpenCoroutine::<usize, usize, usize>::current().is_some());
            1
        }
        assert!(OpenCoroutine::<usize, usize, usize>::current().is_none());
        let mut coroutine =
            OpenCoroutine::new(context_func, 0, 2048).expect("create coroutine failed !");
        coroutine.resume_with(0).as_return().unwrap();
    }
}
