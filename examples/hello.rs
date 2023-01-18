use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine1] launched");
            input
        },
        None,
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            input
        },
        None,
        4096,
    );
    std::thread::sleep(Duration::from_millis(50));
    println!("scheduler finished successfully!");
}
