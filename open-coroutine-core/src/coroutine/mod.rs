use crate::common::page_size;
use crate::coroutine::suspender::SuspenderImpl;
use crate::scheduler::Scheduler;
use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine};
use dashmap::DashMap;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Display, Formatter};
use std::mem::{ManuallyDrop, MaybeUninit};

pub mod suspender;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起到指定时间后继续执行，参数为时间戳
    Suspend(u64),
    ///执行系统调用，参数为系统调用名
    SystemCall(&'static str),
    ///执行用户函数完成
    Complete,
}

impl Display for CoroutineState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

#[repr(C)]
pub struct Coroutine<'c, Param, Yield, Return> {
    name: &'c str,
    sp: ScopedCoroutine<'c, Param, Yield, Return, DefaultStack>,
    state: Cell<CoroutineState>,
    context: DashMap<&'c str, *mut c_void>,
    yields: MaybeUninit<ManuallyDrop<Yield>>,
    //调用用户函数的返回值
    result: MaybeUninit<ManuallyDrop<Return>>,
    scheduler: Option<*const Scheduler>,
}

impl<'c, Param, Yield, Return> Drop for Coroutine<'c, Param, Yield, Return> {
    fn drop(&mut self) {
        //for test_yield case
        if self.sp.started() && !self.sp.done() {
            unsafe { self.sp.force_reset() };
        }
    }
}

unsafe impl<'c, Param, Yield, Return> Send for Coroutine<'c, Param, Yield, Return> {}

#[macro_export]
macro_rules! co {
    ($f:expr, $size:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from(uuid::Uuid::new_v4().to_string()), $f, $size)
            .expect("create coroutine failed !")
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr, $size:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from($name), $f, $size)
            .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            Box::from($name),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
}

thread_local! {
    static COROUTINE: RefCell<*const c_void> = RefCell::new(std::ptr::null());
}

impl<'c, Param, Yield, Return> Coroutine<'c, Param, Yield, Return> {
    pub fn new<F>(name: Box<str>, f: F, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&SuspenderImpl<Param, Yield>, Param) -> Return,
        F: 'c,
    {
        let stack = DefaultStack::new(size.max(page_size()))?;
        let sp = ScopedCoroutine::with_stack(stack, |y, p| {
            let suspender = SuspenderImpl::new(y);
            SuspenderImpl::<Param, Yield>::init_current(&suspender);
            let r = f(&suspender, p);
            SuspenderImpl::<Param, Yield>::clean_current();
            r
        });
        Ok(Coroutine {
            name: Box::leak(name),
            sp,
            state: Cell::new(CoroutineState::Created),
            context: DashMap::new(),
            yields: MaybeUninit::uninit(),
            result: MaybeUninit::uninit(),
            scheduler: None,
        })
    }

    #[allow(clippy::ptr_as_ptr)]
    fn init_current(coroutine: &Coroutine<'c, Param, Yield, Return>) {
        COROUTINE.with(|c| {
            _ = c.replace(coroutine as *const _ as *const c_void);
        });
    }

    #[must_use]
    pub fn current() -> Option<&'c Coroutine<'c, Param, Yield, Return>> {
        COROUTINE.with(|boxed| {
            let ptr = *boxed
                .try_borrow_mut()
                .expect("coroutine current already borrowed");
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr).cast::<Coroutine<'c, Param, Yield, Return>>() })
            }
        })
    }

    fn clean_current() {
        COROUTINE.with(|boxed| {
            *boxed
                .try_borrow_mut()
                .expect("coroutine current already borrowed") = std::ptr::null();
        });
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn put<V>(&self, key: &'c str, val: V) -> Option<V> {
        let v = Box::leak(Box::new(val));
        self.context
            .insert(key, (v as *mut V).cast::<c_void>())
            .map(|ptr| unsafe { *Box::from_raw(ptr.cast::<V>()) })
    }

    pub fn get<V>(&self, key: &'c str) -> Option<&V> {
        self.context
            .get(key)
            .map(|ptr| unsafe { &*ptr.cast::<V>() })
    }

    pub fn get_mut<V>(&self, key: &'c str) -> Option<&mut V> {
        self.context
            .get(key)
            .map(|ptr| unsafe { &mut *ptr.cast::<V>() })
    }

    pub fn remove<V>(&self, key: &'c str) -> Option<V> {
        self.context
            .remove(key)
            .map(|ptr| unsafe { *Box::from_raw(ptr.1.cast::<V>()) })
    }

    pub fn get_state(&self) -> CoroutineState {
        self.state.get()
    }

    pub fn set_state(&self, state: CoroutineState) -> CoroutineState {
        let old = self.state.replace(state);
        crate::info!("co {} change state {}->{}", self.get_name(), old, state);
        old
    }

    pub fn is_finished(&self) -> bool {
        self.get_state() == CoroutineState::Complete
    }

    pub fn get_result(&self) -> Option<Return> {
        if self.is_finished() {
            unsafe {
                let mut m = self.result.assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            }
        } else {
            None
        }
    }

    pub fn get_yield(&self) -> Option<Yield> {
        match self.get_state() {
            CoroutineState::SystemCall(_) | CoroutineState::Suspend(_) => unsafe {
                let mut m = self.yields.assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            },
            _ => None,
        }
    }

    pub fn get_scheduler(&self) -> Option<*const Scheduler> {
        self.scheduler
    }

    pub(crate) fn set_scheduler(&mut self, scheduler: &Scheduler) -> Option<*const Scheduler> {
        self.scheduler.replace(scheduler)
    }

    pub fn resume_with(&mut self, arg: Param) -> CoroutineState {
        let mut current = self.get_state();
        match current {
            CoroutineState::Complete => {
                return CoroutineState::Complete;
            }
            CoroutineState::SystemCall(_) => {}
            CoroutineState::Created | CoroutineState::Ready | CoroutineState::Suspend(0) => {
                current = CoroutineState::Running;
                _ = self.set_state(current);
            }
            _ => panic!("{} unexpected state {current}", self.get_name()),
        };
        Coroutine::<Param, Yield, Return>::init_current(self);
        let state = match self.sp.resume(arg) {
            CoroutineResult::Return(r) => {
                self.result = MaybeUninit::new(ManuallyDrop::new(r));
                let state = CoroutineState::Complete;
                assert_eq!(CoroutineState::Running, self.set_state(state));
                state
            }
            CoroutineResult::Yield(y) => {
                self.yields = MaybeUninit::new(ManuallyDrop::new(y));
                let mut current = self.get_state();
                match current {
                    CoroutineState::Running => {
                        current = CoroutineState::Suspend(SuspenderImpl::<Yield, Param>::timestamp());
                        assert_eq!(CoroutineState::Running, self.set_state(current));
                        current
                    }
                    CoroutineState::SystemCall(syscall_name) => {
                        CoroutineState::SystemCall(syscall_name)
                    }
                    _ => panic!("{} unexpected state {current}", self.get_name()),
                }
            }
        };
        Coroutine::<Param, Yield, Return>::clean_current();
        state
    }
}

impl<'c, Yield, Return> Coroutine<'c, (), Yield, Return> {
    pub fn resume(&mut self) -> CoroutineState {
        self.resume_with(())
    }
}

impl<'c, Param, Yield, Return> Debug for Coroutine<'c, Param, Yield, Return> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("status", &self.state)
            .field("context", &self.context)
            .field("scheduler", &self.scheduler)
            .finish()
    }
}

impl<'c, Param, Yield, Return> Eq for Coroutine<'c, Param, Yield, Return> {}

impl<'c, Param, Yield, Return> PartialEq<Self> for Coroutine<'c, Param, Yield, Return> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(other.name)
    }
}

impl<'c, Param, Yield, Return> PartialOrd<Self> for Coroutine<'c, Param, Yield, Return> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'c, Param, Yield, Return> Ord for Coroutine<'c, Param, Yield, Return> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(other.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unbreakable;

    #[test]
    fn test_return() {
        let mut coroutine = co!(|_s: &SuspenderImpl<'_, i32, ()>, param| {
            assert_eq!(0, param);
            1
        });
        assert_eq!(CoroutineState::Complete, coroutine.resume_with(0));
        assert_eq!(Some(1), coroutine.get_result());
    }

    #[test]
    fn test_yield_once() {
        let mut coroutine = co!(|suspender, param| {
            assert_eq!(1, param);
            _ = suspender.suspend_with(2);
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
    }

    #[test]
    fn test_syscall() {
        let mut coroutine = co!(|suspender, param| {
            assert_eq!(1, param);
            unbreakable!(
                {
                    assert_eq!(3, suspender.suspend_with(2));
                    assert_eq!(5, suspender.suspend_with(4));
                },
                "read"
            );
            if let Some(co) = Coroutine::<i32, i32, i32>::current() {
                assert_eq!(CoroutineState::Running, co.get_state());
            }
            6
        });
        assert_eq!(CoroutineState::SystemCall("read"), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
        assert_eq!(CoroutineState::SystemCall("read"), coroutine.resume_with(3));
        assert_eq!(Some(4), coroutine.get_yield());
        assert_eq!(CoroutineState::Complete, coroutine.resume_with(5));
        assert_eq!(Some(6), coroutine.get_result());
    }

    #[test]
    fn test_yield() {
        let mut coroutine = co!(|suspender, input| {
            assert_eq!(1, input);
            assert_eq!(3, suspender.suspend_with(2));
            assert_eq!(5, suspender.suspend_with(4));
            6
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(3));
        assert_eq!(Some(4), coroutine.get_yield());
        assert_eq!(CoroutineState::Complete, coroutine.resume_with(5));
        assert_eq!(Some(6), coroutine.get_result());
    }

    #[test]
    fn test_current() {
        assert!(Coroutine::<i32, i32, i32>::current().is_none());
        let mut coroutine = co!(|_: &SuspenderImpl<'_, i32, i32>, input| {
            assert_eq!(0, input);
            assert!(Coroutine::<i32, i32, i32>::current().is_some());
            1
        });
        assert_eq!(CoroutineState::Complete, coroutine.resume_with(0));
        assert_eq!(Some(1), coroutine.get_result());
    }

    #[test]
    fn test_backtrace() {
        let mut coroutine = co!(|suspender, input| {
            assert_eq!(1, input);
            println!("{:?}", backtrace::Backtrace::new());
            assert_eq!(3, suspender.suspend_with(2));
            println!("{:?}", backtrace::Backtrace::new());
            4
        });
        assert_eq!(CoroutineState::Suspend(0), coroutine.resume_with(1));
        assert_eq!(Some(2), coroutine.get_yield());
        assert_eq!(CoroutineState::Complete, coroutine.resume_with(3));
        assert_eq!(Some(4), coroutine.get_result());
    }

    #[test]
    fn test_context() {
        let mut coroutine = co!(|_: &SuspenderImpl<'_, (), ()>, ()| {
            let current = Coroutine::<(), (), ()>::current().unwrap();
            assert_eq!(2, *current.get("1").unwrap());
            *current.get_mut("1").unwrap() = 3;
            ()
        });
        assert!(coroutine.put("1", 1).is_none());
        assert_eq!(Some(1), coroutine.put("1", 2));
        assert_eq!(CoroutineState::Complete, coroutine.resume());
        assert_eq!(Some(()), coroutine.get_result());
        assert_eq!(Some(3), coroutine.remove("1"));
    }
}
