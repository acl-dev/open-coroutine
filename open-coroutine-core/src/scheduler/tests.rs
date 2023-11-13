use super::*;
use crate::coroutine::suspender::{SimpleDelaySuspender, SimpleSuspender};
use std::time::Duration;

#[test]
fn test_simple() {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |_, _| {
            println!("1");
            None
        },
        None,
    );
    _ = scheduler.submit_co(
        |_, _| {
            println!("2");
            None
        },
        None,
    );
    scheduler.try_schedule();
}

#[test]
fn test_backtrace() {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(|_, _| None, None);
    _ = scheduler.submit_co(
        |_, _| {
            println!("{:?}", backtrace::Backtrace::new());
            None
        },
        None,
    );
    scheduler.try_schedule();
}

#[test]
fn with_suspend() {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine1] suspend");
            suspender.suspend();
            println!("[coroutine1] back");
            None
        },
        None,
    );
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine2] suspend");
            suspender.suspend();
            println!("[coroutine2] back");
            None
        },
        None,
    );
    scheduler.try_schedule();
}

#[test]
fn with_delay() {
    let scheduler = SchedulerImpl::default();
    _ = scheduler.submit_co(
        |suspender, _| {
            println!("[coroutine] delay");
            suspender.delay(Duration::from_millis(100));
            println!("[coroutine] back");
            None
        },
        None,
    );
    scheduler.try_schedule();
    std::thread::sleep(Duration::from_millis(100));
    scheduler.try_schedule();
}

#[cfg(all(unix, feature = "preemptive-schedule"))]
#[test]
fn preemptive_schedule() -> std::io::Result<()> {
    use std::sync::{Arc, Condvar, Mutex};
    static mut TEST_FLAG1: bool = true;
    static mut TEST_FLAG2: bool = true;
    let pair = Arc::new((Mutex::new(true), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let handler = std::thread::Builder::new()
        .name("test_preemptive_schedule".to_string())
        .spawn(move || {
            let scheduler = Box::leak(Box::new(SchedulerImpl::default()));
            _ = scheduler.submit_co(
                |_, _| {
                    unsafe {
                        while TEST_FLAG1 {
                            _ = libc::usleep(10_000);
                        }
                    }
                    None
                },
                None,
            );
            _ = scheduler.submit_co(
                |_, _| {
                    unsafe {
                        while TEST_FLAG2 {
                            _ = libc::usleep(10_000);
                        }
                    }
                    unsafe { TEST_FLAG1 = false };
                    None
                },
                None,
            );
            _ = scheduler.submit_co(
                |_, _| {
                    unsafe { TEST_FLAG2 = false };
                    None
                },
                None,
            );
            scheduler.try_schedule();

            let (lock, cvar) = &*pair2;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            // notify the condvar that the value has changed.
            cvar.notify_one();
        })
        .expect("failed to spawn thread");

    // wait for the thread to start up
    let (lock, cvar) = &*pair;
    let result = cvar
        .wait_timeout_while(
            lock.lock().unwrap(),
            Duration::from_millis(3000),
            |&mut pending| pending,
        )
        .unwrap();
    if result.1.timed_out() {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "preemptive schedule failed",
        ))
    } else {
        unsafe {
            handler.join().unwrap();
            assert!(!TEST_FLAG1, "preemptive schedule failed");
        }
        Ok(())
    }
}
