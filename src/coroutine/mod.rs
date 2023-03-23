use crate::coroutine::context::Context;
use crate::scheduler::Scheduler;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};

pub mod set_jmp;

pub mod context;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum State {
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
    sp: Context<R>,
    state: Cell<State>,
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
            state: Cell::new(State::Created),
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

    pub fn resume(&self) {
        if !self.sp.is_finished() {
            self.set_state(State::Running);
            ScopedCoroutine::init_current(self);
            self.sp.resume();
            if self.sp.is_finished() {
                self.set_state(State::Finished);
            }
        }
    }

    pub fn suspend(&self) {
        self.set_state(State::Suspend(0));
        ScopedCoroutine::<R>::clean_current();
        self.sp.suspend();
        ScopedCoroutine::<R>::init_current(self);
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn get_state(&self) -> State {
        self.state.get()
    }

    pub(crate) fn set_state(&self, state: State) {
        self.state.set(state);
    }

    pub fn is_finished(&self) -> bool {
        self.get_state() == State::Finished
    }

    pub fn get_result(&self) -> Option<R> {
        if self.is_finished() {
            self.sp.get_result()
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
        co.resume();
        assert!(co.is_finished());
    }

    #[test]
    fn test_yield_once() {
        let co = co!(|co| {
            println!("test_yield_once");
            co.suspend();
        });
        co.resume();
        assert!(!co.is_finished());
    }

    #[test]
    fn test_current() {
        assert!(ScopedCoroutine::<()>::current().is_none());
        let co: ScopedCoroutine<_> = co!(|_| async move {
            assert!(ScopedCoroutine::<()>::current().is_some());
        });
        co.resume();
    }
}
