use base_coroutine::coroutine::UserFunc;
use base_coroutine::EventLoop;
use std::os::raw::c_void;

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
    match EventLoop::submit(f, param, stack_size) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

///轮询协程
#[no_mangle]
pub extern "C" fn try_timed_schedule(ns_time: u64) -> libc::c_int {
    let timeout_time = timer_utils::add_timeout_time(ns_time);
    match EventLoop::round_robin_timeout_schedule(timeout_time) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn timed_schedule(ns_time: u64) -> libc::c_int {
    let timeout_time = timer_utils::add_timeout_time(ns_time);
    match EventLoop::round_robin_timed_schedule(timeout_time) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
