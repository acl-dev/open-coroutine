use open_coroutine::task;
use open_coroutine_core::common::now;

pub fn sleep_test_co(millis: u64) {
    _ = task!(
        move |_| {
            let start = now();
            #[cfg(unix)]
            std::thread::sleep(std::time::Duration::from_millis(millis));
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::System::Threading::Sleep(millis as u32);
            }
            let end = now();
            assert!(end - start >= millis, "Time consumption less than expected");
            println!("[coroutine1] {millis} launched");
        },
        (),
    );
    _ = task!(
        move |_| {
            #[cfg(unix)]
            std::thread::sleep(std::time::Duration::from_millis(500));
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::System::Threading::Sleep(500);
            }
            println!("[coroutine2] {millis} launched");
        },
        (),
    );
    #[cfg(unix)]
    std::thread::sleep(std::time::Duration::from_millis(millis + 500));
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Threading::Sleep((millis + 500) as u32);
    }
}

#[open_coroutine::main(event_loop_size = 1, max_size = 2)]
pub fn main() {
    sleep_test_co(1);
    sleep_test_co(1000);
}
