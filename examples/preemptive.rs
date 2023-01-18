use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

fn main() {
    static mut FLAG: bool = true;
    let handle = co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine1] launched");
            unsafe {
                while FLAG {
                    println!("loop");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            input
        },
        None,
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            unsafe {
                FLAG = false;
            }
            input
        },
        None,
        4096,
    );
    let _ = handle.join();
    unsafe { assert!(!FLAG) };
    println!("preemptive schedule finished successfully!");
}
