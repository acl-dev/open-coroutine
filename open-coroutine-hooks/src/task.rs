use open_coroutine_core::common::JoinHandler;
use open_coroutine_core::net::event_loop::join::TaskJoinHandle;
use open_coroutine_core::net::event_loop::{EventLoops, UserFunc};
use std::ffi::{c_long, c_void};
use std::time::Duration;

///创建任务
#[no_mangle]
pub extern "C" fn task_crate(f: UserFunc, param: usize) -> TaskJoinHandle {
    EventLoops::submit(
        move |suspender, p| {
            #[allow(clippy::cast_ptr_alignment, clippy::ptr_as_ptr)]
            Some(f(std::ptr::from_ref(suspender), p.unwrap_or(0)))
        },
        Some(param),
    )
}

///等待任务完成
#[no_mangle]
pub extern "C" fn task_join(handle: TaskJoinHandle) -> c_long {
    match handle.join() {
        Ok(ptr) => match ptr {
            Ok(ptr) => match ptr {
                Some(ptr) => ptr as *mut c_void as c_long,
                None => 0,
            },
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}

///等待任务完成
#[no_mangle]
pub extern "C" fn task_timeout_join(handle: &TaskJoinHandle, ns_time: u64) -> c_long {
    match handle.timeout_join(Duration::from_nanos(ns_time)) {
        Ok(ptr) => match ptr {
            Ok(ptr) => match ptr {
                Some(ptr) => ptr as *mut c_void as c_long,
                None => 0,
            },
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}
