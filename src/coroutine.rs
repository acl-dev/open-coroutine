use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use uuid::Uuid;

pub use genawaiter::{
    sync::{Co, Gen},
    GeneratorState,
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
    ///执行用户函数完成
    Finished,
}

thread_local! {
    static RESULT: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

fn init_result<R>(result: R) {
    RESULT.with(|boxed| {
        let mut r = ManuallyDrop::new(result);
        *boxed.borrow_mut() = &mut r as *mut _ as *mut c_void;
    })
}

fn take_result<R>() -> Option<R> {
    RESULT.with(|boxed| {
        let ptr = *boxed.borrow_mut();
        if ptr.is_null() {
            None
        } else {
            unsafe {
                let r = Some(ManuallyDrop::take(&mut *(ptr as *mut ManuallyDrop<R>)));
                *boxed.borrow_mut() = std::ptr::null_mut();
                r
            }
        }
    })
}

#[repr(C)]
pub struct Coroutine<'a, Y, R, F: Future> {
    name: &'a str,
    sp: RefCell<Gen<Y, (), F>>,
    status: Cell<Status>,
    result: PhantomData<R>,
}

#[macro_export]
macro_rules! co {
    ($name:literal, $func:expr) => {{
        let gen = Gen::new(|co| async move {
            let result = ($func)(co).await;
            init_result(result);
        });
        Coroutine {
            name: Box::leak(Box::from($name)),
            sp: RefCell::new(gen),
            status: Cell::new(Status::Created),
            result: Default::default(),
        }
    }};
    ($func:expr) => {{
        let gen = Gen::new(|co| async move {
            let result = ($func)(co).await;
            init_result(result);
        });
        Coroutine {
            name: Box::leak(Box::from(Uuid::new_v4().to_string())),
            sp: RefCell::new(gen),
            status: Cell::new(Status::Created),
            result: Default::default(),
        }
    }};
}

impl<Y, R, F: Future> Coroutine<'_, Y, R, F> {
    pub fn resume(&self) -> GeneratorState<Y, R> {
        self.set_status(Status::Running);
        let state = self.sp.borrow_mut().resume();
        match state {
            GeneratorState::Yielded(y) => {
                self.set_status(Status::Suspend);
                GeneratorState::Yielded(y)
            }
            GeneratorState::Complete(_r) => {
                self.set_status(Status::Finished);
                GeneratorState::Complete(take_result().unwrap())
            }
        }
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
        let co: Coroutine<'_, (), _, _> = co!(|_| async move {});
        assert_eq!(GeneratorState::Complete(()), co.resume());
    }

    #[test]
    fn test_yield() {
        let s = "hello";
        let co = co!(|co: Co<_, ()>| async move {
            co.yield_(10).await;
            println!("{}", s);
            co.yield_(20).await;
            "world"
        });
        assert_eq!(co.resume(), GeneratorState::Yielded(10));
        assert_eq!(co.resume(), GeneratorState::Yielded(20));
        assert_eq!(co.resume(), GeneratorState::Complete("world"));
    }
}
