use open_coroutine_core::coroutine::UserFunc;
use open_coroutine_core::{EventLoop, JoinHandle, OpenYielder};
use std::os::raw::c_void;

#[no_mangle]
pub extern "C" fn init_hook() {
    //啥都不做，只是为了保证hook的函数能够被重定向到
    //主要为了防止有的程序压根不调用coroutine_crate的情况
}

///创建协程
/// fixme
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunc<&'static mut c_void, (), &'static mut c_void>,
    param: &'static mut c_void,
    stack_size: usize,
) -> JoinHandle {
    match EventLoop::submit(
        move |_yielder, input| {
            let yielder = OpenYielder::<&'static mut c_void, ()>::yielder();
            unsafe { f(Box::leak(Box::from_raw(yielder)), input) }
        },
        param,
        stack_size,
    ) {
        Ok(handle) => handle,
        Err(_) => JoinHandle::error(),
    }
}

///等待协程完成
/// fixme
#[allow(improper_ctypes_definitions)]
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
    match handle.timeout_join(std::time::Duration::from_nanos(ns_time)) {
        Ok(ptr) => match ptr {
            Some(ptr) => ptr as *mut c_void as libc::c_long,
            None => 0,
        },
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
