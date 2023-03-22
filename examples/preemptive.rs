use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    static mut EXAMPLE_FLAG1: bool = true;
    static mut EXAMPLE_FLAG2: bool = true;
    let handle = co(
        |_yielder, input: Option<&'static mut i32>| {
            println!("[coroutine1] launched");
            while unsafe { EXAMPLE_FLAG1 } {
                println!("loop1");
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("loop1 end");
            input
        },
        Some(Box::leak(Box::new(1))),
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut i32>| {
            println!("[coroutine2] launched");
            while unsafe { EXAMPLE_FLAG2 } {
                println!("loop2");
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("loop2 end");
            unsafe { EXAMPLE_FLAG1 = false };
            input
        },
        Some(Box::leak(Box::new(1))),
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine3] launched");
            unsafe { EXAMPLE_FLAG2 = false };
            input
        },
        None,
        4096,
    );
    let result = handle.join().unwrap().unwrap() as *mut c_void as *mut i32;
    unsafe {
        assert_eq!(std::ptr::read_unaligned(result), 1);
        assert!(!EXAMPLE_FLAG1);
    }
    println!("preemptive schedule finished successfully!");
}
