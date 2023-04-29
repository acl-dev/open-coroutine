use super::*;
use std::time::Duration;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
        .as_nanos() as u64
}

fn hook_test(millis: u64) {
    _ = co!(
        |_, _| {
            println!("[coroutine1] launched");
        },
        (),
        4096,
    );
    _ = co!(
        |_, _| {
            println!("[coroutine2] launched");
        },
        (),
        4096,
    );
    let start = now();
    std::thread::sleep(Duration::from_millis(millis));
    let end = now();
    assert!(end - start >= millis);
}

fn hook_test_co(millis: u64) {
    _ = co!(
        |_, _| {
            let start = now();
            std::thread::sleep(Duration::from_millis(millis));
            let end = now();
            assert!(end - start >= millis);
            println!("[coroutine1] launched");
        },
        (),
        4096,
    );
    _ = co!(
        |_, _| {
            std::thread::sleep(Duration::from_millis(500));
            println!("[coroutine2] launched");
        },
        (),
        4096,
    );
    std::thread::sleep(Duration::from_millis(millis + 500));
}

#[test]
fn hook_test_schedule_timeout() {
    hook_test(1)
}

#[test]
fn hook_test_schedule_normal() {
    hook_test(1_000)
}

#[test]
fn hook_test_co_schedule_timeout() {
    hook_test_co(1)
}

#[test]
fn hook_test_co_schedule_normal() {
    hook_test_co(1_000)
}
