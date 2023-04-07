use crate::coroutine::suspender::Suspender;
use crate::scheduler::Scheduler;
use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod suspender;

#[allow(clippy::pedantic)]
pub fn page_size() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);
    if ret == 0 {
        unsafe {
            cfg_if::cfg_if! {
                if #[cfg(windows)] {
                    let mut info = std::mem::zeroed();
                    windows_sys::Win32::System::SystemInformation::GetSystemInfo(&mut info);
                    ret = info.dwPageSize as usize
                } else {
                    ret = libc::sysconf(libc::_SC_PAGESIZE) as usize;
                }
            }
        }
        PAGE_SIZE.store(ret, Ordering::Relaxed);
    }
    ret
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起，参数为开始执行的时间戳
    Suspend(u64),
    ///执行系统调用
    SystemCall,
    ///栈扩/缩容时
    CopyStack,
    ///执行用户函数完成
    Finished,
}

#[repr(C)]
pub struct Coroutine<'c, Param, Yield, Return> {
    name: &'c str,
    sp: RefCell<ScopedCoroutine<'c, Param, Yield, (), DefaultStack>>,
    state: Cell<CoroutineState>,
    yields: RefCell<MaybeUninit<ManuallyDrop<Yield>>>,
    //调用用户函数的返回值
    result: RefCell<MaybeUninit<ManuallyDrop<Return>>>,
    scheduler: RefCell<Option<*const Scheduler>>,
}

impl<'c, Param, Yield, Return> Drop for Coroutine<'c, Param, Yield, Return> {
    fn drop(&mut self) {
        //for test_yield case
        let mut sp = self.sp.borrow_mut();
        if sp.started() && !sp.done() {
            unsafe { sp.force_reset() };
        }
    }
}

unsafe impl<'c, Param, Yield, Return> Send for Coroutine<'c, Param, Yield, Return> {}

#[macro_export]
macro_rules! co {
    ($f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            $crate::coroutine::page_size() * 8,
        )
        .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr, $size:literal $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from($name), $f, $size)
            .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from($name), $f, $crate::coroutine::page_size() * 8)
            .expect("create coroutine failed !")
    };
}

thread_local! {
    static COROUTINE: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl<'c, Param, Yield, Return> Coroutine<'c, Param, Yield, Return> {
    pub fn new<F>(name: Box<str>, f: F, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&Suspender<Param, Yield>, Param) -> Return,
        F: 'c,
    {
        let stack = DefaultStack::new(size)?;
        let sp = ScopedCoroutine::with_stack(stack, |y, p| {
            let suspender = Suspender::new(y);
            Suspender::<Param, Yield>::init_current(&suspender);
            let r = f(&suspender, p);
            Suspender::<Param, Yield>::clean_current();
            let current = Coroutine::<Param, Yield, Return>::current().unwrap();
            current.set_state(CoroutineState::Finished);
            let _ = current
                .result
                .replace(MaybeUninit::new(ManuallyDrop::new(r)));
            if let Some(_scheduler) = current.get_scheduler() {}
        });
        Ok(Coroutine {
            name: Box::leak(name),
            sp: RefCell::new(sp),
            state: Cell::new(CoroutineState::Created),
            yields: RefCell::new(MaybeUninit::uninit()),
            result: RefCell::new(MaybeUninit::uninit()),
            scheduler: RefCell::new(None),
        })
    }

    #[allow(clippy::ptr_as_ptr)]
    fn init_current(coroutine: &Coroutine<'c, Param, Yield, Return>) {
        COROUTINE.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        });
    }

    #[must_use]
    pub fn current() -> Option<&'c Coroutine<'c, Param, Yield, Return>> {
        COROUTINE.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr).cast::<Coroutine<'c, Param, Yield, Return>>() })
            }
        })
    }

    fn clean_current() {
        COROUTINE.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn get_state(&self) -> CoroutineState {
        self.state.get()
    }

    pub fn set_state(&self, state: CoroutineState) {
        self.state.set(state);
    }

    pub fn is_finished(&self) -> bool {
        self.get_state() == CoroutineState::Finished
    }

    pub fn get_result(&self) -> Option<Return> {
        if self.is_finished() {
            unsafe {
                let mut m = self.result.borrow().assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            }
        } else {
            None
        }
    }

    pub fn get_yield(&self) -> Option<Yield> {
        match self.get_state() {
            CoroutineState::Suspend(_) => unsafe {
                let mut m = self.yields.borrow().assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            },
            _ => None,
        }
    }

    pub fn get_scheduler(&self) -> Option<*const Scheduler> {
        *self.scheduler.borrow()
    }

    pub(crate) fn set_scheduler(&self, scheduler: &Scheduler) -> Option<*const Scheduler> {
        self.scheduler.replace(Some(scheduler))
    }

    pub fn resume_with(&self, arg: Param) -> CoroutineState {
        if self.is_finished() {
            return CoroutineState::Finished;
        }
        self.set_state(CoroutineState::Running);
        Coroutine::<Param, Yield, Return>::init_current(self);
        let state = match self.sp.borrow_mut().resume(arg) {
            CoroutineResult::Return(_) => CoroutineState::Finished,
            CoroutineResult::Yield(y) => {
                let state = CoroutineState::Suspend(Suspender::<Yield, Param>::timestamp());
                self.set_state(state);
                let _ = self.yields.replace(MaybeUninit::new(ManuallyDrop::new(y)));
                Suspender::<Yield, Param>::clean_timestamp();
                state
            }
        };
        Coroutine::<Param, Yield, Return>::clean_current();
        state
    }
}

impl<'c, Yield, Return> Coroutine<'c, (), Yield, Return> {
    pub fn resume(&self) -> CoroutineState {
        self.resume_with(())
    }
}

impl<'c, Param, Yield, Return> Debug for Coroutine<'c, Param, Yield, Return> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("status", &self.state)
            .field("scheduler", &self.scheduler)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return() {
        let coroutine = co!(|_s: &Suspender<'_, i32, ()>, param| {
            assert_eq!(0, param);
            1
        });
        assert_eq!(CoroutineState::Finished, coroutine.resume_with(0));
        assert_eq!(Some(1), coroutine.get_result());
    }

    #[test]
    fn test_yield_once() {
        let coroutine = co!(|yielder, param| {
            assert_eq!(1, param);
            let _ = yielder.suspend_with(2);
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
    }

    #[test]
    fn test_yield() {
        let coroutine = co!(|yielder, input| {
            assert_eq!(1, input);
            assert_eq!(3, yielder.suspend_with(2));
            assert_eq!(5, yielder.suspend_with(4));
            6
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(3));
        assert_eq!(Some(4), coroutine.get_yield());
        assert_eq!(CoroutineState::Finished, coroutine.resume_with(5));
        assert_eq!(Some(6), coroutine.get_result());
    }

    #[test]
    fn test_current() {
        assert!(Coroutine::<i32, i32, i32>::current().is_none());
        let coroutine = co!(|_yielder: &Suspender<'_, i32, i32>, input| {
            assert_eq!(0, input);
            assert!(Coroutine::<i32, i32, i32>::current().is_some());
            1
        });
        assert_eq!(CoroutineState::Finished, coroutine.resume_with(0));
        assert_eq!(Some(1), coroutine.get_result());
    }

    #[test]
    fn test_backtrace() {
        let coroutine = co!(|yielder, input| {
            assert_eq!(1, input);
            println!("{:?}", backtrace::Backtrace::new());
            assert_eq!(3, yielder.suspend_with(2));
            println!("{:?}", backtrace::Backtrace::new());
            4
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
        assert_eq!(CoroutineState::Finished, coroutine.resume_with(3));
        assert_eq!(Some(4), coroutine.get_result());
    }
}
