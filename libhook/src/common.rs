use crate::event_loop::EventLoop;
use base_coroutine::coroutine::UserFunc;
use std::os::raw::c_void;
use std::time::Duration;

#[no_mangle]
pub extern "C" fn init_hook() {
    //啥都不做，只是为了保证hook的函数能够被重定向到
    //主要为了防止有的程序压根不调用coroutine_crate的情况
}

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunc<&'static mut c_void, (), &'static mut c_void>,
    param: &'static mut c_void,
    stack_size: usize,
) -> libc::c_int {
    match EventLoop::next_scheduler().submit(f, param, stack_size) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

///轮询协程
#[no_mangle]
pub extern "C" fn try_timed_schedule(ns_time: u64) -> libc::c_int {
    match EventLoop::next_scheduler().try_timed_schedule(Duration::from_nanos(ns_time)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn timed_schedule(ns_time: u64) -> libc::c_int {
    match EventLoop::next_scheduler().timed_schedule(Duration::from_nanos(ns_time)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
