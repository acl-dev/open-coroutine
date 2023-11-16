use open_coroutine_core::coroutine::suspender::SuspenderImpl;
use open_coroutine_core::net::event_loop::join::JoinHandle;
use open_coroutine_core::net::event_loop::{EventLoops, UserFunc};
use std::ffi::{c_long, c_void};
use std::time::Duration;

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(f: UserFunc, param: usize, stack_size: usize) -> JoinHandle {
    let _stack_size = if stack_size > 0 {
        Some(stack_size)
    } else {
        None
    };
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

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_join(handle: JoinHandle) -> c_long {
    match handle.join() {
        Ok(ptr) => match ptr {
            Some(ptr) => match ptr {
                Ok(ptr) => match ptr {
                    Some(ptr) => ptr as *mut c_void as c_long,
                    None => 0,
                },
                Err(_) => -1,
            },
            None => -1,
        },
        Err(_) => -1,
    }
}

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_timeout_join(handle: &JoinHandle, ns_time: u64) -> c_long {
    match handle.timeout_join(Duration::from_nanos(ns_time)) {
        Ok(ptr) => match ptr {
            Some(ptr) => match ptr {
                Ok(ptr) => match ptr {
                    Some(ptr) => ptr as *mut c_void as c_long,
                    None => 0,
                },
                Err(_) => -1,
            },
            None => -1,
        },
        Err(_) => -1,
    }
}
