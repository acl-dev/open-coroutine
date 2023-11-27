use open_coroutine_core::common::JoinHandle;
use open_coroutine_core::coroutine::suspender::SuspenderImpl;
use open_coroutine_core::net::event_loop::join::TaskJoinHandleImpl;
use open_coroutine_core::net::event_loop::{EventLoops, TaskFunc};
use std::ffi::{c_long, c_void};
use std::time::Duration;

///创建任务
#[no_mangle]
pub extern "C" fn task_crate(f: TaskFunc, param: usize) -> TaskJoinHandleImpl {
    EventLoops::submit(
        move |suspender, p| {
            #[allow(clippy::cast_ptr_alignment, clippy::ptr_as_ptr)]
            Some(f(
                suspender as *const _ as *const SuspenderImpl<(), ()>,
                p.unwrap_or(0),
            ))
        },
        Some(param),
    )
}

///等待任务完成
#[no_mangle]
pub extern "C" fn task_join(handle: TaskJoinHandleImpl) -> c_long {
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
pub extern "C" fn task_timeout_join(handle: &TaskJoinHandleImpl, ns_time: u64) -> c_long {
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
