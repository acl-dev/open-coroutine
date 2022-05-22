use std::os::raw::{c_int, c_void};
use libfiber::fiber::{ACL_FIBER, acl_fiber_create, acl_fiber_kill, acl_fiber_schedule_stop, acl_fiber_schedule_with, FIBER_EVENT_KERNEL};

unsafe extern "C" fn fiber_main(fiber: *mut ACL_FIBER, arg: *mut c_void) {
    println!("arg:{}", arg as usize);
    acl_fiber_kill(fiber);
}

fn main() {
    unsafe {
        acl_fiber_create(Some(fiber_main), 1 as *mut c_void, 128 * 1024 * 1024);
        acl_fiber_schedule_with(FIBER_EVENT_KERNEL as c_int);
        acl_fiber_schedule_stop();
    }
    println!("Hello, world!");
}