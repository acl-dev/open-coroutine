use open_coroutine::task;
use open_coroutine_core::common::now;

fn sleep_test(millis: u64) {
    _ = task!(
        move |_| {
            println!("[coroutine1] {millis} launched");
        },
        (),
    );
    _ = task!(
        move |_| {
            println!("[coroutine2] {millis} launched");
        },
        (),
    );
    let start = now();
    #[cfg(unix)]
    std::thread::sleep(std::time::Duration::from_millis(millis));
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Threading::Sleep(millis as u32);
    }
    let end = now();
    assert!(end - start >= millis, "Time consumption less than expected");
}

#[open_coroutine::main(event_loop_size = 1, max_size = 2)]
pub fn main() {
    sleep_test(1);
    sleep_test(1000);
}
