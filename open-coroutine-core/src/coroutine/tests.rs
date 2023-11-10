use crate::common::{Current, Named};
use crate::constants::{CoroutineState, Syscall, SyscallState};
use crate::coroutine::local::HasCoroutineLocal;
use crate::coroutine::suspender::Suspender;
use crate::coroutine::CoroutineImpl;
use crate::unbreakable;

#[test]
fn test_return() {
    let mut coroutine = co!(|_s: &dyn Suspender<Resume = i32, Yield = ()>, param| {
        assert_eq!(0, param);
        1
    });
    assert_eq!(CoroutineState::Complete(1), coroutine.resume_with(0));
    assert_eq!(Some(1), coroutine.get_result());
}

#[test]
fn test_yield_once() {
    let mut coroutine = co!(|suspender, param| {
        assert_eq!(1, param);
        _ = suspender.suspend_with(2);
    });
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1));
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
            read
        );
        if let Some(co) = CoroutineImpl::<i32, i32, i32>::current() {
            assert_eq!(CoroutineState::Running, co.state());
        }
        6
    });
    matches!(
        coroutine.resume_with(1),
        CoroutineState::SystemCall(_, Syscall::read, SyscallState::Executing),
    );
    assert_eq!(Some(2), coroutine.get_yield());
    matches!(
        coroutine.resume_with(3),
        CoroutineState::SystemCall(_, Syscall::read, SyscallState::Executing),
    );
    assert_eq!(Some(4), coroutine.get_yield());
    assert_eq!(CoroutineState::Complete(6), coroutine.resume_with(5));
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
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1));
    assert_eq!(Some(2), coroutine.get_yield());
    assert_eq!(CoroutineState::Suspend(4, 0), coroutine.resume_with(3));
    assert_eq!(Some(4), coroutine.get_yield());
    assert_eq!(CoroutineState::Complete(6), coroutine.resume_with(5));
    assert_eq!(Some(6), coroutine.get_result());
}

#[test]
fn test_current() {
    assert!(CoroutineImpl::<i32, i32, i32>::current().is_none());
    let mut coroutine = co!(|_: &dyn Suspender<Resume = i32, Yield = i32>, input| {
        assert_eq!(0, input);
        assert!(CoroutineImpl::<i32, i32, i32>::current().is_some());
        1
    });
    assert_eq!(CoroutineState::Complete(1), coroutine.resume_with(0));
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
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1));
    assert_eq!(Some(2), coroutine.get_yield());
    assert_eq!(CoroutineState::Complete(4), coroutine.resume_with(3));
    assert_eq!(Some(4), coroutine.get_result());
}

#[test]
fn test_context() {
    let mut coroutine = co!(|_: &dyn Suspender<Resume = (), Yield = ()>, ()| {
        let current = CoroutineImpl::<(), (), ()>::current().unwrap();
        assert_eq!(2, *current.get("1").unwrap());
        *current.get_mut("1").unwrap() = 3;
        ()
    });
    assert!(coroutine.put("1", 1).is_none());
    assert_eq!(Some(1), coroutine.put("1", 2));
    assert_eq!(CoroutineState::Complete(()), coroutine.resume());
    assert_eq!(Some(()), coroutine.get_result());
    assert_eq!(Some(3), coroutine.remove("1"));
}
