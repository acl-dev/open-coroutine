use std::os::raw::{c_int, c_void};
use libfiber::fiber::{Fiber, Scheduler};
use libfiber::libfiber::{ACL_FIBER, acl_fiber_create, acl_fiber_kill, acl_fiber_schedule_stop, acl_fiber_schedule_with, FIBER_EVENT_KERNEL};

fn fiber_main(fiber: &Fiber, arg: Option<*mut c_void>) {
    println!("arg:{}", arg.unwrap() as usize);
}

fn main() {
    Fiber::new(fiber_main, Some(1 as *mut c_void), 128 * 1024 * 1024);
    let scheduler = Scheduler::new(FIBER_EVENT_KERNEL);
    scheduler.start();
    scheduler.stop();
    println!("Hello, world!");
}