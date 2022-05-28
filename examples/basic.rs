use std::os::raw::{c_int, c_void};
use libfiber::fiber::Fiber;
use libfiber::scheduler::{EventMode, Scheduler};

fn fiber_main(fiber: &Fiber, arg: Option<*mut c_void>) {
    match arg {
        Some(arg) => println!("arg:{}", arg as usize),
        None => println!("no param")
    }
}

fn main() {
    Fiber::new(fiber_main, Some(1 as *mut c_void), 128 * 1024 * 1024);
    let scheduler = Scheduler::new(EventMode::Kernel);
    scheduler.start();
    scheduler.stop();
    println!("Hello, world!");
}