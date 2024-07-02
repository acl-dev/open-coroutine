use crate::common::{Current, Named};
use crate::constants::{Syscall, SyscallState};
use crate::net::event_loop::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use crate::syscall::common::{reset_errno, set_errno};
use libc::timespec;
use once_cell::sync::Lazy;
use open_coroutine_timer::get_timeout_time;
use std::ffi::c_int;
use std::time::Duration;

#[must_use]
pub extern "C" fn nanosleep(
    fn_ptr: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
    rqtp: *const timespec,
    rmtp: *mut timespec,
) -> c_int {
    static CHAIN: Lazy<NanosleepSyscallFacade<NioNanosleepSyscall>> = Lazy::new(Default::default);
    CHAIN.nanosleep(fn_ptr, rqtp, rmtp)
}

trait NanosleepSyscall {
    extern "C" fn nanosleep(
        &self,
        fn_ptr: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
        rqtp: *const timespec,
        rmtp: *mut timespec,
    ) -> c_int;
}

impl_facade!(NanosleepSyscallFacade, NanosleepSyscall,
    nanosleep(rqtp: *const timespec, rmtp: *mut timespec) -> c_int
);

#[derive(Debug, Copy, Clone, Default)]
struct NioNanosleepSyscall {}

impl NanosleepSyscall for NioNanosleepSyscall {
    extern "C" fn nanosleep(
        &self,
        _: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
        rqtp: *const timespec,
        rmtp: *mut timespec,
    ) -> c_int {
        let rqtp = unsafe { *rqtp };
        if rqtp.tv_sec < 0 || rqtp.tv_nsec < 0 || rqtp.tv_nsec > 999_999_999 {
            set_errno(libc::EINVAL);
            return -1;
        }
        let time = Duration::new(rqtp.tv_sec as u64, rqtp.tv_nsec as u32);
        if let Some(co) = SchedulableCoroutine::current() {
            let syscall = Syscall::nanosleep;
            let new_state = SyscallState::Suspend(get_timeout_time(time));
            if co.syscall((), syscall, new_state).is_err() {
                crate::error!(
                    "{} change to syscall {} {} failed !",
                    co.get_name(),
                    syscall,
                    new_state
                );
            }
        }
        //等待事件到来
        _ = EventLoops::wait_just(Some(time));
        reset_errno();
        if !rmtp.is_null() {
            unsafe {
                (*rmtp).tv_sec = 0;
                (*rmtp).tv_nsec = 0;
            }
        }
        0
    }
}
