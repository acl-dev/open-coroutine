use std::cell::{Cell, RefCell};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::mem::{ManuallyDrop, MaybeUninit};
use uuid::Uuid;

pub use genawaiter::{
    sync::{gen, Co, Gen},
    yield_, GeneratorState,
};

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
    ///调用用户函数完成，但未退出
    Finished,
    ///已退出
    Exited,
}

#[repr(C)]
pub struct Coroutine<'a, Y, R, F: Future> {
    name: &'a str,
    sp: RefCell<Gen<Y, (), F>>,
    status: Cell<Status>,
    result: RefCell<MaybeUninit<ManuallyDrop<R>>>,
}

#[macro_export]
macro_rules! co {
    ($name:literal, $body:expr) => {
        Coroutine {
            name: Box::leak(Box::from($name)),
            sp: RefCell::new(genawaiter::sync::gen!($body)),
            status: Cell::new(Status::Created),
            result: RefCell::new(MaybeUninit::uninit()),
        }
    };
    ($body:expr) => {
        Coroutine {
            name: Box::leak(Box::from(Uuid::new_v4().to_string())),
            sp: RefCell::new(genawaiter::sync::gen!($body)),
            status: Cell::new(Status::Created),
            result: RefCell::new(MaybeUninit::uninit()),
        }
    };
}

impl<Y, R, F: Future> Coroutine<'_, Y, R, F> {
    pub fn resume(&self) -> GeneratorState<Y, F::Output> {
        self.set_status(Status::Ready);
        self.sp.borrow_mut().resume_with(())
    }

    pub fn set_status(&self, status: Status) {
        self.status.set(status);
    }
}

impl<Y, R, F: Future> Debug for Coroutine<'_, Y, R, F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("status", &self.status)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return() {
        let co: Coroutine<'_, i32, (), _> = co!({});
        assert_eq!(GeneratorState::Complete(()), co.resume());
    }

    #[test]
    fn test_yield() {
        let co: Coroutine<'_, i32, (), _> = co!({
            yield_!(1);
            yield_!(2);
            yield_!(3);
        });
        assert_eq!(GeneratorState::Yielded(1), co.resume());
        assert_eq!(GeneratorState::Yielded(2), co.resume());
        assert_eq!(GeneratorState::Yielded(3), co.resume());
        assert_eq!(GeneratorState::Complete(()), co.resume());
    }
}
