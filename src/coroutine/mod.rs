use crate::coroutine::suspend::Suspender;
use crate::scheduler::Scheduler;
use std::cell::{Cell, RefCell};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::pin::Pin;

use genawaiter::stack::Gen;
pub use genawaiter::{stack::Co, GeneratorState};

mod result;

pub mod suspend;

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum State {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起
    Suspend(u64),
    ///执行系统调用
    SystemCall,
    ///执行用户函数完成
    Finished,
}

#[repr(C)]
pub struct Coroutine<'c, 's, Y, R, F>
where
    F: genawaiter::Coroutine<Yield = Y, Resume = (), Return = ()>,
{
    name: &'c str,
    sp: RefCell<F>,
    state: Cell<State>,
    result: PhantomData<R>,
    scheduler: RefCell<Option<&'c Scheduler<'s>>>,
}

#[macro_export]
macro_rules! co {
    // Safety: The goal here is to ensure the safety invariants of `Gen::new`, i.e.,
    // the lifetime of the `Co` argument (in `$producer`) must not outlive `shelf`
    // or `generator`.
    //
    // We create two variables, `shelf` and `generator`, which cannot be named by
    // user-land code (because of macro hygiene). Because they are declared in the
    // same scope, and cannot be dropped before the end of the scope (because they
    // cannot be named), they have equivalent lifetimes. The type signature of
    // `Gen::new` ties the lifetime of `co` to that of `shelf`. This means it has
    // the same lifetime as `generator`, and so the invariant of `Gen::new` cannot
    // be violated.
    ($var_name:ident, $func:expr $(,)?) => {
        let shelf = Box::leak(Box::new(genawaiter::stack::Shelf::new()));
        let generator = unsafe {
            Gen::new(shelf, |co| async move {
                let result = ($func)(Suspender::new(co)).await;
                result::init_result(result);
            })
        };
        let mut coroutine = Coroutine::new(Box::from(uuid::Uuid::new_v4().to_string()), generator);
        let $var_name = &mut coroutine;
    };
    ($var_name:ident, $name:literal, $func:expr $(,)?) => {
        let shelf = Box::leak(Box::new(genawaiter::stack::Shelf::new()));
        let generator = unsafe {
            Gen::new(shelf, |co| async move {
                let result = ($func)(Suspender::new(co)).await;
                result::init_result(result);
            })
        };
        let mut coroutine = Coroutine::new(Box::from($name), generator);
        let $var_name = &mut coroutine;
    };
}

impl<'c, 's, Y, R, F> Coroutine<'c, 's, Y, R, F>
where
    F: genawaiter::Coroutine<Yield = Y, Resume = (), Return = ()> + Unpin,
{
    fn new(name: Box<str>, generator: F) -> Self {
        Coroutine {
            name: Box::leak(name),
            sp: RefCell::new(generator),
            state: Cell::new(State::Created),
            result: Default::default(),
            scheduler: RefCell::new(None),
        }
    }

    pub fn resume(&self) -> GeneratorState<Y, R> {
        self.set_state(State::Running);
        let mut binding = self.sp.borrow_mut();
        let sp = Pin::new(binding.deref_mut());
        match sp.resume_with(()) {
            GeneratorState::Yielded(y) => {
                if Suspender::<Y, R>::syscall_flag() {
                    self.set_state(State::SystemCall);
                    Suspender::<Y, R>::clean_syscall_flag();
                } else {
                    self.set_state(State::Suspend(Suspender::<Y, R>::delay_time()));
                    Suspender::<Y, R>::clean_delay();
                }
                GeneratorState::Yielded(y)
            }
            GeneratorState::Complete(_r) => {
                self.set_state(State::Finished);
                GeneratorState::Complete(result::take_result().unwrap())
            }
        }
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

    pub fn get_scheduler(&self) -> Option<&'c Scheduler<'s>> {
        *self.scheduler.borrow()
    }

    pub(crate) fn set_scheduler(&self, scheduler: &'s Scheduler<'s>) -> Option<&'c Scheduler<'s>> {
        self.scheduler.replace(Some(scheduler))
    }
}

impl<Y, R, F> Debug for Coroutine<'_, '_, Y, R, F>
where
    F: genawaiter::Coroutine<Yield = Y, Resume = (), Return = ()>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("state", &self.state)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genawaiter::stack::let_gen_using;

    #[test]
    fn base() {
        let s = "1";
        let f = || async move {
            print!("{} ", s);
            "2"
        };
        let_gen_using!(gen, |co| async move {
            co.yield_(10).await;
            println!("{}", f().await);
            co.yield_(20).await;
        });
        assert_eq!(gen.resume(), GeneratorState::Yielded(10));
        assert_eq!(gen.resume(), GeneratorState::Yielded(20));
        assert_eq!(gen.resume(), GeneratorState::Complete(()));
    }

    #[test]
    fn test_return() {
        co!(co, |_| async move {});
        let co = co as &mut Coroutine<'_, '_, (), _, _>;
        assert_eq!(GeneratorState::Complete(()), co.resume());
    }

    #[test]
    fn test_yield() {
        let s = "hello";
        co!(c, "test", |co: Suspender<'static, _, _>| async move {
            co.suspend(10).await;
            println!("{}", s);
            co.suspend(20).await;
            "world"
        });
        assert_eq!(c.resume(), GeneratorState::Yielded(10));
        assert_eq!(c.resume(), GeneratorState::Yielded(20));
        assert_eq!(c.resume(), GeneratorState::Complete("world"));
        assert_eq!(c.get_name(), "test");
    }
}
