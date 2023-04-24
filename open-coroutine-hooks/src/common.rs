use open_coroutine_core::event_loop::join::JoinHandle;
use open_coroutine_core::event_loop::{EventLoops, UserFunc};
use std::ffi::c_void;
use std::time::Duration;

#[no_mangle]
pub extern "C" fn init_hook() {
    //啥都不做，只是为了保证hook的函数能够被重定向到
    //防止压根不调用coroutine_crate的情况
}

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunc,
    param: &'static mut c_void,
    stack_size: usize,
) -> JoinHandle {
    let stack_size = if stack_size > 0 {
        Some(stack_size)
    } else {
        None
    };
    match EventLoops::submit(move |suspender, _| f(suspender, param), stack_size) {
        Ok(handle) => handle,
        Err(_) => JoinHandle::error(),
    }
}

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_join(handle: JoinHandle) -> libc::c_long {
    match handle.join() {
        Ok(ptr) => match ptr {
            Some(ptr) => ptr as *mut c_void as libc::c_long,
            None => 0,
        },
        Err(_) => -1,
    }
}

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_timeout_join(handle: &JoinHandle, ns_time: u64) -> libc::c_long {
    match handle.timeout_join(Duration::from_nanos(ns_time)) {
        Ok(ptr) => match ptr {
            Some(ptr) => ptr as *mut c_void as libc::c_long,
            None => 0,
        },
        Err(_) => -1,
    }
}

///轮询协程
#[no_mangle]
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
pub extern "C" fn try_timed_schedule(ns_time: u64) -> libc::c_int {
    let timeout_time = open_coroutine_timer::add_timeout_time(ns_time);
    match EventLoops::try_timeout_schedule(timeout_time) {
        Ok(left_time) => left_time as libc::c_int,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn timed_schedule(ns_time: u64) -> libc::c_int {
    match EventLoops::wait_event(Some(Duration::from_nanos(ns_time))) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
