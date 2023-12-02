#[no_mangle]
pub extern "C" fn sleep(secs: libc::c_uint) -> libc::c_uint {
    open_coroutine_core::syscall::sleep(None, secs)
}

#[no_mangle]
pub extern "C" fn usleep(secs: libc::c_uint) -> libc::c_int {
    open_coroutine_core::syscall::usleep(None, secs)
}

#[no_mangle]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub extern "C" fn nanosleep(rqtp: *const libc::timespec, rmtp: *mut libc::timespec) -> libc::c_int {
    open_coroutine_core::syscall::nanosleep(None, rqtp, rmtp)
}
