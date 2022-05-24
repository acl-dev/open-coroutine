use std::os::raw::{c_int, c_void};
use libfiber::fiber::{Fiber, Scheduler};
use libfiber::libfiber::{ACL_FIBER, acl_fiber_create, acl_fiber_kill, acl_fiber_schedule_stop, acl_fiber_schedule_with, FIBER_EVENT_KERNEL};

///todo 继续往下简化
unsafe extern "C" fn fiber_main(fiber: *mut ACL_FIBER, arg: *mut c_void) {
    println!("arg:{}", arg as usize);
    acl_fiber_kill(fiber);
}

fn main() {
    Fiber::new(fiber_main, 1 as *mut c_void, 128 * 1024 * 1024);
    let scheduler = Scheduler::new(FIBER_EVENT_KERNEL);
    scheduler.start();
    scheduler.stop();
    println!("Hello, world!");
}