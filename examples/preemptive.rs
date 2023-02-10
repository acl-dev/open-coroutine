use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    static mut EXAMPLE_FLAG: bool = true;
    let handle = co(
        |_yielder, input: Option<&'static mut i32>| {
            println!("[coroutine1] launched");
            while unsafe { EXAMPLE_FLAG } {
                println!("loop");
                std::thread::sleep(Duration::from_millis(10));
            }
            input
        },
        Some(Box::leak(Box::new(1))),
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            unsafe { EXAMPLE_FLAG = false };
            input
        },
        None,
        4096,
    );
    let result = handle.join().unwrap().unwrap() as *mut c_void as *mut i32;
    unsafe {
        assert_eq!(1, std::ptr::read_unaligned(result));
        assert!(!EXAMPLE_FLAG);
    }
    println!("preemptive schedule finished successfully!");
}
