use corosensei::stack::{DefaultStack, Stack};
use open_coroutine_core::co;
use open_coroutine_core::common::constants::CoroutineState;
use open_coroutine_core::coroutine::suspender::Suspender;
use open_coroutine_core::coroutine::Coroutine;

#[test]
fn coroutine_basic() -> std::io::Result<()> {
    let mut coroutine = co!(|suspender, input| {
        assert_eq!(1, input);
        assert_eq!(3, suspender.suspend_with(2));
        4
    })?;
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1)?);
    assert_eq!(CoroutineState::Complete(4), coroutine.resume_with(3)?);
    Ok(())
}

#[test]
fn coroutine_panic() -> std::io::Result<()> {
    let mut coroutine = co!(|_: &Suspender<'_, (), ()>, ()| {
        panic!("test panic, just ignore it");
    })?;
    let result = coroutine.resume()?;
    let error = match result {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
    Ok(())
}

#[test]
fn coroutine_backtrace() -> std::io::Result<()> {
    let mut coroutine = co!(|suspender, input| {
        assert_eq!(1, input);
        println!("{:?}", backtrace::Backtrace::new());
        assert_eq!(3, suspender.suspend_with(2));
        println!("{:?}", backtrace::Backtrace::new());
        4
    })?;
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1)?);
    assert_eq!(CoroutineState::Complete(4), coroutine.resume_with(3)?);
    Ok(())
}

#[test]
fn coroutine_delay() -> std::io::Result<()> {
    let mut coroutine = co!(|s, ()| {
        let current = Coroutine::<(), (), ()>::current().unwrap();
        assert_eq!(CoroutineState::Running, current.state());
        s.delay(std::time::Duration::MAX);
        unreachable!();
    })?;
    assert_eq!(CoroutineState::Ready, coroutine.state());
    assert_eq!(CoroutineState::Suspend((), u64::MAX), coroutine.resume()?);
    assert_eq!(CoroutineState::Suspend((), u64::MAX), coroutine.state());
    assert_eq!(
        format!(
            "{} unexpected {}->{:?}",
            coroutine.name(),
            CoroutineState::<(), ()>::Suspend((), u64::MAX),
            CoroutineState::<(), ()>::Running
        ),
        coroutine.resume().unwrap_err().to_string()
    );
    assert_eq!(CoroutineState::Suspend((), u64::MAX), coroutine.state());
    Ok(())
}

#[test]
fn sp_in_bounds() -> std::io::Result<()> {
    let mut coroutine = co!(|suspender, input| {
        let current = Coroutine::<(), (), ()>::current().unwrap();
        if let Some(stack_info) = current.stack_infos().back().copied() {
            assert!(current.stack_ptr_in_bounds(psm::stack_pointer() as u64));
            assert_eq!(
                current.stack_ptr_in_bounds(stack_info.stack_top as u64 + 1),
                false
            );
            assert_eq!(
                current.stack_ptr_in_bounds(stack_info.stack_bottom as u64 - 1),
                false
            );
        }
        assert_eq!(1, input);
        assert_eq!(3, suspender.suspend_with(2));
        4
    })?;
    assert_eq!(CoroutineState::Suspend(2, 0), coroutine.resume_with(1)?);
    assert_eq!(CoroutineState::Complete(4), coroutine.resume_with(3)?);
    println!("{:?}", coroutine);
    println!("{}", coroutine);
    Ok(())
}

#[test]
fn thread_stack_growth() {
    fn recurse(i: u32, p: &mut [u8; 10240]) {
        Coroutine::<(), (), ()>::maybe_grow(|| {
            // Ensure the stack allocation isn't optimized away.
            unsafe { std::ptr::read_volatile(&p) };
            if i > 0 {
                recurse(i - 1, &mut [0; 10240]);
            }
        })
        .expect("allocate stack failed")
    }
    // Use 10MB of stack.
    recurse(1000, &mut [0; 10240]);
}

#[test]
fn coroutine_stack_growth() -> std::io::Result<()> {
    let mut coroutine = co!(|_: &Suspender<(), ()>, ()| {
        fn recurse(i: u32, p: &mut [u8; 10240]) {
            Coroutine::<(), (), ()>::maybe_grow(|| {
                // Ensure the stack allocation isn't optimized away.
                unsafe { std::ptr::read_volatile(&p) };
                if i > 0 {
                    recurse(i - 1, &mut [0; 10240]);
                }
            })
            .expect("allocate stack failed")
        }

        let stack = DefaultStack::new(open_coroutine_core::common::constants::DEFAULT_STACK_SIZE)
            .expect("allocate stack failed");
        let max_remaining = stack.base().get() - stack.limit().get();
        // Use ~500KB of stack.
        recurse(50, &mut [0; 10240]);
        let remaining_stack = unsafe {
            Coroutine::<(), (), ()>::current()
                .unwrap()
                .remaining_stack()
        };
        assert!(
            remaining_stack < max_remaining,
            "remaining stack {remaining_stack} when max {max_remaining}"
        );
        // Use ~500KB of stack.
        recurse(50, &mut [0; 10240]);
        let remaining_stack = unsafe {
            Coroutine::<(), (), ()>::current()
                .unwrap()
                .remaining_stack()
        };
        assert!(
            remaining_stack < max_remaining,
            "remaining stack {remaining_stack} when max {max_remaining}"
        );
    })?;
    assert_eq!(coroutine.resume()?, CoroutineState::Complete(()));
    Ok(())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86", feature = "preemptive")))]
#[test]
fn coroutine_trap() -> std::io::Result<()> {
    let mut coroutine = co!(|_: &Suspender<'_, (), ()>, ()| {
        println!("Before trap");
        unsafe { std::ptr::write_volatile(1 as *mut u8, 0) };
        println!("After trap");
    })?;
    let result = coroutine.resume()?;
    let error = match result {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
    Ok(())
}

#[cfg(not(any(
    debug_assertions,
    all(target_os = "linux", target_arch = "x86", feature = "preemptive")
)))]
#[test]
fn coroutine_invalid_memory_reference() -> std::io::Result<()> {
    let mut coroutine = co!(|_: &Suspender<'_, (), ()>, ()| {
        println!("Before invalid memory reference");
        // 没有加--release运行，会收到SIGABRT信号，不好处理，直接禁用测试
        unsafe {
            let co = &*((1usize as *mut std::ffi::c_void).cast::<Coroutine<(), (), ()>>());
            println!("{}", co.state());
        }
        println!("After invalid memory reference");
    })?;
    let result = coroutine.resume();
    assert!(result.is_ok());
    println!("{:?}", result);
    let error = match result.unwrap() {
        CoroutineState::Error(_) => true,
        _ => false,
    };
    assert!(error);
    Ok(())
}

#[cfg(all(unix, feature = "preemptive"))]
#[test]
fn coroutine_preemptive() -> std::io::Result<()> {
    let pair = std::sync::Arc::new((std::sync::Mutex::new(true), std::sync::Condvar::new()));
    let pair2 = pair.clone();
    _ = std::thread::Builder::new()
        .name("preemptive".to_string())
        .spawn(move || {
            let mut coroutine: Coroutine<(), (), ()> = co!(|_, ()| { loop {} })?;
            assert_eq!(CoroutineState::Suspend((), 0), coroutine.resume()?);
            assert_eq!(CoroutineState::Suspend((), 0), coroutine.state());
            // should execute to here
            let (lock, cvar) = &*pair2;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            cvar.notify_one();
            Ok::<(), std::io::Error>(())
        });
    // wait for the thread to start up
    let (lock, cvar) = &*pair;
    let result = cvar
        .wait_timeout_while(
            lock.lock().unwrap(),
            std::time::Duration::from_millis(3000),
            |&mut pending| pending,
        )
        .unwrap();
    if result.1.timed_out() {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "The monitor should send signals to coroutines in running state",
        ))
    } else {
        Ok(())
    }
}

#[cfg(all(unix, feature = "preemptive"))]
#[test]
fn coroutine_syscall_not_preemptive() -> std::io::Result<()> {
    use open_coroutine_core::common::constants::{Syscall, SyscallState};

    let pair = std::sync::Arc::new((std::sync::Mutex::new(true), std::sync::Condvar::new()));
    let pair2 = pair.clone();
    _ = std::thread::Builder::new()
        .name("syscall_not_preemptive".to_string())
        .spawn(move || {
            let mut coroutine: Coroutine<(), (), ()> = co!(|_, ()| {
                Coroutine::<(), (), ()>::current()
                    .unwrap()
                    .syscall((), Syscall::sleep, SyscallState::Executing)
                    .unwrap();
                loop {}
            })?;
            _ = coroutine.resume()?;
            // should never execute to here
            let (lock, cvar) = &*pair2;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            cvar.notify_one();
            Ok::<(), std::io::Error>(())
        });
    // wait for the thread to start up
    let (lock, cvar) = &*pair;
    let result = cvar
        .wait_timeout_while(
            lock.lock().unwrap(),
            std::time::Duration::from_millis(1000),
            |&mut pending| pending,
        )
        .unwrap();
    if result.1.timed_out() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "The monitor should not send signals to coroutines in syscall state",
        ))
    }
}
