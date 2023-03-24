use crate::coroutine::context::{Context, GeneratorState};
use crate::scheduler::Scheduler;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::mem::{ManuallyDrop, MaybeUninit};

pub mod set_jmp;

pub mod context;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起，参数为延迟的时间，单位ns
    Suspend(u64),
    ///执行系统调用
    SystemCall,
    ///执行用户函数完成
    Finished,
}

#[repr(C)]
pub struct ScopedCoroutine<'c, 's, R> {
    name: &'c str,
    sp: Context<'c, R>,
    state: Cell<CoroutineState>,
    result: RefCell<MaybeUninit<ManuallyDrop<R>>>,
    scheduler: RefCell<Option<&'c Scheduler<'s>>>,
}

unsafe impl<R> Send for ScopedCoroutine<'_, '_, R> {}

impl<R> Debug for ScopedCoroutine<'_, '_, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("state", &self.state)
            .finish()
    }
}

#[macro_export]
macro_rules! co {
    ($f:expr $(,)?) => {
        $crate::coroutine::ScopedCoroutine::new(Box::from(uuid::Uuid::new_v4().to_string()), $f)
    };
    ($name:literal, $f:expr $(,)?) => {
        $crate::coroutine::ScopedCoroutine::new(Box::from($name), $f)
    };
}

impl<'c, R: 'static> ScopedCoroutine<'c, '_, R> {
    pub fn new(name: Box<str>, f: impl FnOnce(&ScopedCoroutine<R>) -> R + 'static) -> Self {
        ScopedCoroutine {
            name: Box::leak(name),
            sp: Context::new(move |_| {
                let current = ScopedCoroutine::<R>::current().unwrap();
                let r = f(current);
                ScopedCoroutine::<R>::clean_current();
                r
            }),
            state: Cell::new(CoroutineState::Created),
            result: RefCell::new(MaybeUninit::uninit()),
            scheduler: RefCell::new(None),
        }
    }
}

thread_local! {
    static COROUTINE: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl<'c, 's, R> ScopedCoroutine<'c, 's, R> {
    #[allow(clippy::pedantic)]
    fn init_current(coroutine: &ScopedCoroutine<'c, 's, R>) {
        COROUTINE.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        });
    }

    #[must_use]
    pub fn current() -> Option<&'c ScopedCoroutine<'c, 's, R>> {
        COROUTINE.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr.cast::<ScopedCoroutine<'c, 's, R>>()) })
            }
        })
    }

    fn clean_current() {
        COROUTINE.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    pub fn resume(&self) -> CoroutineState {
        if self.sp.is_finished() {
            return CoroutineState::Finished;
        }
        self.set_state(CoroutineState::Running);
        ScopedCoroutine::init_current(self);
        match self.sp.resume() {
            GeneratorState::Complete(r) => {
                let state = CoroutineState::Finished;
                self.set_state(state);
                let _ = self.result.replace(MaybeUninit::new(ManuallyDrop::new(r)));
                state
            }
            GeneratorState::Yielded => {
                let state = CoroutineState::Suspend(0);
                self.set_state(state);
                state
            }
        }
    }

    pub fn suspend(&self) {
        self.set_state(CoroutineState::Suspend(0));
        ScopedCoroutine::<R>::clean_current();
        self.sp.suspend();
        ScopedCoroutine::<R>::init_current(self);
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn get_state(&self) -> CoroutineState {
        self.state.get()
    }

    pub(crate) fn set_state(&self, state: CoroutineState) {
        self.state.set(state);
    }

    pub fn is_finished(&self) -> bool {
        self.get_state() == CoroutineState::Finished
    }

    pub fn get_result(&self) -> Option<R> {
        if self.is_finished() {
            unsafe {
                let mut m = self.result.borrow().assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            }
        } else {
            None
        }
    }

    pub fn get_scheduler(&self) -> Option<&'c Scheduler<'s>> {
        *self.scheduler.borrow()
    }

    pub(crate) fn set_scheduler(&self, scheduler: &'s Scheduler<'s>) -> Option<&'c Scheduler<'s>> {
        self.scheduler.replace(Some(scheduler))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return() {
        let co = co!(|_| {
            println!("test_return");
        });
        assert_eq!(CoroutineState::Finished, co.resume());
        assert_eq!(Some(()), co.get_result());
    }

    #[test]
    fn test_yield_once() {
        let co = co!(|co| {
            println!("test_yield_once");
            co.suspend();
        });
        assert_eq!(CoroutineState::Suspend(0), co.resume());
        assert_eq!(None, co.get_result());
    }

    #[test]
    fn test_current() {
        assert!(ScopedCoroutine::<()>::current().is_none());
        let co = co!(|_| {
            assert!(ScopedCoroutine::<()>::current().is_some());
        });
        assert_eq!(CoroutineState::Finished, co.resume());
        assert_eq!(Some(()), co.get_result());
    }
}
