use open_coroutine_core::common::JoinHandle;
use open_coroutine_core::coroutine::suspender::SuspenderImpl;
use open_coroutine_core::net::event_loop::join::CoJoinHandleImpl;
use open_coroutine_core::net::event_loop::{EventLoops, UserFunc};
use std::ffi::{c_long, c_void};
use std::time::Duration;

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunc,
    param: usize,
    stack_size: usize,
) -> CoJoinHandleImpl {
    let stack_size = if stack_size > 0 {
        Some(stack_size)
    } else {
        None
    };
    EventLoops::submit_co(
        move |suspender, ()| {
            #[allow(clippy::cast_ptr_alignment, clippy::ptr_as_ptr)]
            Some(f(
                std::ptr::from_ref(suspender) as *const SuspenderImpl<(), ()>,
                param,
            ))
        },
        stack_size,
    )
    .unwrap_or_else(|_| CoJoinHandleImpl::err())
}

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_join(handle: CoJoinHandleImpl) -> c_long {
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

///等待协程完成
#[no_mangle]
pub extern "C" fn coroutine_timeout_join(handle: &CoJoinHandleImpl, ns_time: u64) -> c_long {
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
