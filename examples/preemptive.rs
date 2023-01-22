use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    static mut FLAG: bool = true;
    co(
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
    std::thread::sleep(Duration::from_millis(50));
    unsafe { assert!(!FLAG) };
    println!("preemptive schedule finished successfully!");
}
