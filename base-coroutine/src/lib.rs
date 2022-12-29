#[allow(dead_code)]
mod id;

#[allow(dead_code)]
mod stack;

pub use stack::{Stack, StackError};

#[allow(dead_code)]
mod context;

// export defer
pub use scopeguard::*;

#[allow(dead_code)]
pub mod coroutine;

pub use coroutine::*;

#[allow(dead_code)]
mod work_steal;

pub use work_steal::*;

#[allow(dead_code)]
mod random;

#[allow(dead_code)]
pub mod scheduler;

pub use scheduler::*;

#[allow(dead_code)]
#[cfg(unix)]
mod monitor;

use std::os::raw::c_void;
use std::time::Duration;

type UserFunction<'a> = extern "C" fn(
    &'a Yielder<&'static mut c_void, c_void, &'static mut c_void>,
    &'static mut c_void,
) -> &'static mut c_void;

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunction,
    param: &'static mut c_void,
    stack_size: usize,
) -> libc::c_int {
    match Scheduler::current().submit(f, param, stack_size) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn suspend(
    yielder: &Yielder<&'static mut c_void, c_void, &'static mut c_void>,
) -> &'static mut c_void {
    unsafe { yielder.suspend(std::mem::transmute(0u8)) }
}

#[no_mangle]
pub extern "C" fn delay(
    yielder: &Yielder<&'static mut c_void, c_void, &'static mut c_void>,
    ms_time: u64,
) -> &'static mut c_void {
    unsafe { yielder.delay(std::mem::transmute(0u8), ms_time) }
}

///轮询协程
#[no_mangle]
pub extern "C" fn try_timed_schedule(ms_time: u64) -> libc::c_int {
    match Scheduler::current().try_timed_schedule(Duration::from_millis(ms_time)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn try_timeout_schedule(timeout_time: u64) -> libc::c_int {
    match Scheduler::current().try_timeout_schedule(timeout_time) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
