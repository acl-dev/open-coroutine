use open_coroutine_core::event_loop::EventLoops;
use std::time::Duration;

#[no_mangle]
pub extern "C" fn sleep(secs: libc::c_uint) -> libc::c_uint {
    open_coroutine_core::info!("sleep hooked");
    _ = EventLoops::wait_event(Some(Duration::from_secs(u64::from(secs))));
    crate::unix::reset_errno();
    0
}

#[no_mangle]
pub extern "C" fn usleep(secs: libc::c_uint) -> libc::c_int {
    open_coroutine_core::info!("usleep hooked");
    let time = match u64::from(secs).checked_mul(1_000) {
        Some(v) => Duration::from_nanos(v),
        None => Duration::MAX,
    };
    _ = EventLoops::wait_event(Some(time));
    crate::unix::reset_errno();
    0
}

#[no_mangle]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub extern "C" fn nanosleep(rqtp: *const libc::timespec, rmtp: *mut libc::timespec) -> libc::c_int {
    open_coroutine_core::info!("nanosleep hooked");
    let rqtp = unsafe { *rqtp };
    if rqtp.tv_sec < 0 || rqtp.tv_nsec < 0 || rqtp.tv_nsec > 999_999_999 {
        crate::unix::set_errno(libc::EINVAL);
        return -1;
    }
    //等待事件到来
    _ = EventLoops::wait_event(Some(Duration::new(rqtp.tv_sec as u64, rqtp.tv_nsec as u32)));
    crate::unix::reset_errno();
    if !rmtp.is_null() {
        unsafe {
            (*rmtp).tv_sec = 0;
            (*rmtp).tv_nsec = 0;
        }
    }
    0
}
