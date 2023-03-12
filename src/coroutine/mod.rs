use std::cell::{Cell, RefCell};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;

pub use genawaiter::{stack::Co, GeneratorState};
pub use suspend::*;

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
pub struct Coroutine<'a, Y, R, F: Future> {
    name: &'a str,
    sp: RefCell<genawaiter::stack::Gen<'a, Y, (), F>>,
    state: Cell<State>,
    result: PhantomData<R>,
}

#[macro_export]
macro_rules! co {
    ($name:ident, $func:expr $(,)?) => {
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
        let shelf = Box::leak(Box::new(genawaiter::stack::Shelf::new()));
        let generator = unsafe {
            genawaiter::stack::Gen::new(shelf, |co| async move {
                let result = ($func)(Suspender::new(co)).await;
                result::init_result(result);
            })
        };
        let mut coroutine = Coroutine {
            name: Box::leak(Box::from(uuid::Uuid::new_v4().to_string())),
            sp: RefCell::new(generator),
            state: Cell::new(State::Created),
            result: Default::default(),
        };
        let $name = &mut coroutine;
    };
}

impl<Y, R, F: Future> Coroutine<'_, Y, R, F> {
    pub fn resume(&self) -> GeneratorState<Y, R> {
        self.set_state(State::Running);
        let state = self.sp.borrow_mut().resume();
        match state {
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

    pub(crate) fn set_state(&self, state: State) {
        self.state.set(state);
    }
}

impl<Y, R, F: Future> Debug for Coroutine<'_, Y, R, F> {
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
        let co = co as &mut Coroutine<'_, (), _, _>;
        assert_eq!(GeneratorState::Complete(()), co.resume());
    }

    #[test]
    fn test_yield() {
        let s = "hello";
        co!(c, |co: Suspender<'static, _, _>| async move {
            co.suspend(10).await;
            println!("{}", s);
            co.suspend(20).await;
            "world"
        });
        assert_eq!(c.resume(), GeneratorState::Yielded(10));
        assert_eq!(c.resume(), GeneratorState::Yielded(20));
        assert_eq!(c.resume(), GeneratorState::Complete("world"));
    }
}
