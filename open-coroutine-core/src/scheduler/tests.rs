use super::*;
use crate::constants::Syscall;
use crate::coroutine::suspender::{SimpleDelaySuspender, SimpleSuspender};
use std::time::Duration;

#[test]
fn test_simple() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            println!("1");
            None
        },
        None,
    )?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("2");
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn test_backtrace() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(|_, _| None, None)?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("{:?}", backtrace::Backtrace::new());
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn with_suspend() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine1] suspend");
            suspender.suspend();
            println!("[coroutine1] back");
            None
        },
        None,
    )?;
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine2] suspend");
            suspender.suspend();
            println!("[coroutine2] back");
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}

#[test]
fn with_delay() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine] delay");
            suspender.delay(Duration::from_millis(100));
            println!("[coroutine] back");
            None
        },
        None,
    )?;
    scheduler.try_schedule()?;
    std::thread::sleep(Duration::from_millis(100));
    scheduler.try_schedule()
}

#[test]
fn test_state() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            if let Some(coroutine) = SchedulableCoroutine::current() {
                match coroutine.state() {
                    CoroutineState::Running => println!("syscall nanosleep started !"),
                    _ => unreachable!("test_state 1 should never execute to here"),
                };
                let timeout_time =
                    open_coroutine_timer::get_timeout_time(Duration::from_millis(10));
                coroutine
                    .syscall((), Syscall::nanosleep, SyscallState::Suspend(timeout_time))
                    .expect("change to syscall state failed !");
                if let Some(suspender) = SchedulableSuspender::current() {
                    suspender.suspend();
                }
            }
            if let Some(coroutine) = SchedulableCoroutine::current() {
                match coroutine.state() {
                    CoroutineState::Running => println!("syscall nanosleep finished !"),
                    _ => unreachable!("test_state 2 should never execute to here"),
                };
            }
            None
        },
        None,
    )?;
    scheduler.try_schedule()?;
    std::thread::sleep(Duration::from_millis(10));
    scheduler.try_schedule()
}

#[cfg(not(all(
    target_os = "linux",
    target_arch = "aarch64",
    feature = "preemptive-schedule"
)))]
#[test]
fn test_trap() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            println!("Before trap");
            unsafe { std::ptr::write_volatile(1 as *mut u8, 0) };
            println!("After trap");
            None
        },
        None,
    )?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("200");
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}

#[cfg(not(debug_assertions))]
#[test]
fn test_invalid_memory_reference() -> std::io::Result<()> {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            println!("Before invalid memory reference");
            // 没有加--release运行，会收到SIGABRT信号，不好处理，直接禁用测试
            unsafe { _ = &*((1usize as *mut std::ffi::c_void).cast::<SchedulableCoroutine>()) };
            println!("After invalid memory reference");
            None
        },
        None,
    )?;
    _ = scheduler.submit_co(
        |_, _| {
            println!("200");
            None
        },
        None,
    )?;
    scheduler.try_schedule()
}
