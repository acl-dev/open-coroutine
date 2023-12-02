use libc::timespec;
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};

static SLEEP: Lazy<extern "C" fn(c_uint) -> c_uint> = init_hook!("sleep");

#[no_mangle]
pub extern "C" fn sleep(secs: c_uint) -> c_uint {
    open_coroutine_core::syscall::sleep(Some(Lazy::force(&SLEEP)), secs)
}

static USLEEP: Lazy<extern "C" fn(c_uint) -> c_int> = init_hook!("usleep");

#[no_mangle]
pub extern "C" fn usleep(secs: c_uint) -> c_int {
    open_coroutine_core::syscall::usleep(Some(Lazy::force(&USLEEP)), secs)
}

static NANOSLEEP: Lazy<extern "C" fn(*const timespec, *mut timespec) -> c_int> =
    init_hook!("nanosleep");

#[no_mangle]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub extern "C" fn nanosleep(rqtp: *const timespec, rmtp: *mut timespec) -> c_int {
    open_coroutine_core::syscall::nanosleep(Some(Lazy::force(&NANOSLEEP)), rqtp, rmtp)
}
