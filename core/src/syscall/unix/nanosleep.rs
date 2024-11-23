use crate::net::EventLoops;
use crate::syscall::{reset_errno, set_errno};
use libc::timespec;
use once_cell::sync::Lazy;
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

#[repr(C)]
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
        let time = Duration::new(
            rqtp.tv_sec.try_into().expect("overflow"),
            rqtp.tv_nsec.try_into().expect("overflow")
        );
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            let syscall = crate::common::constants::Syscall::nanosleep;
            let new_state = crate::common::constants::SyscallState::Suspend(
                crate::common::get_timeout_time(time),
            );
            if co.syscall((), syscall, new_state).is_err() {
                crate::error!(
                    "{} change to syscall {} {} failed !",
                    co.name(),
                    syscall,
                    new_state
                );
            }
        }
        //等待事件到来
        _ = EventLoops::wait_event(Some(time));
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
