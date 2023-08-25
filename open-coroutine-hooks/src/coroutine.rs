use open_coroutine_core::event_loop::join::JoinHandle;
use open_coroutine_core::event_loop::{EventLoops, UserFunc};
use std::ffi::c_void;
use std::time::Duration;

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(f: UserFunc, param: usize, stack_size: usize) -> JoinHandle {
    let _stack_size = if stack_size > 0 {
        Some(stack_size)
    } else {
        None
    };
    EventLoops::submit(move |suspender, ()| f(suspender, param))
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
