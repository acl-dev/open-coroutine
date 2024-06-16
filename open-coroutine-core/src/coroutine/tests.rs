use super::*;

#[test]
fn test_return() {
    let mut coroutine = co!(|_: &dyn Suspender<'_, Yield = (), Resume = i32>, param| {
        assert_eq!(0, param);
        1
    });
    assert_eq!(
        CoroutineState::Complete(1),
        coroutine.resume_with(0).unwrap()
    );
}

#[test]
fn test_yield_once() {
    let mut coroutine = co!(
        |suspender: &dyn Suspender<'_, Resume = i32, Yield = i32>, param| {
            assert_eq!(1, param);
            _ = suspender.suspend_with(2);
        }
    );
    assert_eq!(
        CoroutineState::Suspend(2, 0),
        coroutine.resume_with(1).unwrap()
    );
}

#[test]
fn test_yield() {
    let mut coroutine = co!(|suspender, input| {
        assert_eq!(1, input);
        assert_eq!(3, suspender.suspend_with(2));
        assert_eq!(5, suspender.suspend_with(4));
        6
    });
    assert_eq!(
        CoroutineState::Suspend(2, 0),
        coroutine.resume_with(1).unwrap()
    );
    assert_eq!(
        CoroutineState::Suspend(4, 0),
        coroutine.resume_with(3).unwrap()
    );
    assert_eq!(
        CoroutineState::Complete(6),
        coroutine.resume_with(5).unwrap()
    );
}

#[test]
fn test_current() {
    assert!(CoroutineImpl::<i32, i32, i32>::current().is_none());
    let parent_name = "parent";
    let mut parent = co!(
        String::from(parent_name),
        |_: &dyn Suspender<'_, Resume = i32, Yield = i32>, input| {
            assert_eq!(0, input);
            assert_eq!(
                parent_name,
                CoroutineImpl::<i32, i32, i32>::current()
                    .unwrap()
                    .get_name()
            );
            assert_eq!(
                parent_name,
                CoroutineImpl::<i32, i32, i32>::current()
                    .unwrap()
                    .get_name()
            );

            let child_name = "child";
            let mut child = co!(
                String::from(child_name),
                |_: &dyn Suspender<'_, Resume = i32, Yield = i32>, input| {
                    assert_eq!(0, input);
                    assert_eq!(
                        child_name,
                        CoroutineImpl::<i32, i32, i32>::current()
                            .unwrap()
                            .get_name()
                    );
                    assert_eq!(
                        child_name,
                        CoroutineImpl::<i32, i32, i32>::current()
                            .unwrap()
                            .get_name()
                    );
                    1
                }
            );
            assert_eq!(CoroutineState::Complete(1), child.resume_with(0).unwrap());

            assert_eq!(
                parent_name,
                CoroutineImpl::<i32, i32, i32>::current()
                    .unwrap()
                    .get_name()
            );
            assert_eq!(
                parent_name,
                CoroutineImpl::<i32, i32, i32>::current()
                    .unwrap()
                    .get_name()
            );
            1
        }
    );
    assert_eq!(CoroutineState::Complete(1), parent.resume_with(0).unwrap());
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
    assert_eq!(
        CoroutineState::Suspend(2, 0),
        coroutine.resume_with(1).unwrap()
    );
    assert_eq!(
        CoroutineState::Complete(4),
        coroutine.resume_with(3).unwrap()
    );
}

#[test]
fn test_context() {
    let mut coroutine = co!(|_: &dyn Suspender<'_, Resume = (), Yield = ()>, ()| {
        let current = CoroutineImpl::<(), (), ()>::current().unwrap();
        assert_eq!(2, *current.get("1").unwrap());
        *current.get_mut("1").unwrap() = 3;
        ()
    });
    assert!(coroutine.put("1", 1).is_none());
    assert_eq!(Some(1), coroutine.put("1", 2));
    assert_eq!(CoroutineState::Complete(()), coroutine.resume().unwrap());
    assert_eq!(Some(3), coroutine.remove("1"));
}

#[test]
fn test_panic() {
    let mut coroutine = co!(|_: &dyn Suspender<'_, Yield = (), Resume = ()>, ()| {
        panic!("test panic, just ignore it");
    });
    let result = coroutine.resume();
    assert!(result.is_ok());
    let error = match result.unwrap() {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
}

#[test]
fn test_trap() {
    let mut coroutine = co!(|_: &dyn Suspender<'_, Yield = (), Resume = ()>, ()| {
        println!("Before trap");
        unsafe { std::ptr::write_volatile(1 as *mut u8, 0) };
        println!("After trap");
    });
    let result = coroutine.resume();
    assert!(result.is_ok());
    let error = match result.unwrap() {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
}

#[cfg(not(debug_assertions))]
#[test]
fn test_invalid_memory_reference() {
    let mut coroutine = co!(|_: &dyn Suspender<'_, Yield = (), Resume = ()>, ()| {
        println!("Before invalid memory reference");
        // 没有加--release运行，会收到SIGABRT信号，不好处理，直接禁用测试
        unsafe {
            let co = &*((1usize as *mut std::ffi::c_void).cast::<CoroutineImpl<(), (), ()>>());
            println!("{}", co.state());
        }
        println!("After invalid memory reference");
    });
    let result = coroutine.resume();
    assert!(result.is_ok());
    println!("{:?}", result);
    let error = match result.unwrap() {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
}
