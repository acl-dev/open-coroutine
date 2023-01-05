use open_coroutine::{co, Yielder};
use std::os::raw::c_void;
use std::time::Duration;

extern "C" fn f1(
    _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    _input: Option<&'static mut c_void>,
) -> Option<&'static mut c_void> {
    println!("[coroutine1] launched");
    None
}

extern "C" fn f2(
    _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    _input: Option<&'static mut c_void>,
) -> Option<&'static mut c_void> {
    println!("[coroutine2] launched");
    None
}

fn main() {
    co(f1, None, 4096);
    co(f2, None, 4096);
    std::thread::sleep(Duration::from_millis(10));
    println!("scheduler finished successfully!");
}
