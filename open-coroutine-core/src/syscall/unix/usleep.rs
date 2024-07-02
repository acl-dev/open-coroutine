use crate::common::{Current, Named};
use crate::constants::{Syscall, SyscallState};
use crate::net::event_loop::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use crate::syscall::common::reset_errno;
use once_cell::sync::Lazy;
use open_coroutine_timer::get_timeout_time;
use std::ffi::{c_int, c_uint};
use std::time::Duration;

#[must_use]
pub extern "C" fn usleep(
    fn_ptr: Option<&extern "C" fn(c_uint) -> c_int>,
    microseconds: c_uint,
) -> c_int {
    static CHAIN: Lazy<UsleepSyscallFacade<NioUsleepSyscall>> = Lazy::new(Default::default);
    CHAIN.usleep(fn_ptr, microseconds)
}

trait UsleepSyscall {
    extern "C" fn usleep(
        &self,
        fn_ptr: Option<&extern "C" fn(c_uint) -> c_int>,
        microseconds: c_uint,
    ) -> c_int;
}

impl_facade!(UsleepSyscallFacade, UsleepSyscall,
    usleep(microseconds: c_uint) -> c_int
);

#[derive(Debug, Copy, Clone, Default)]
struct NioUsleepSyscall {}

impl UsleepSyscall for NioUsleepSyscall {
    extern "C" fn usleep(
        &self,
        _: Option<&extern "C" fn(c_uint) -> c_int>,
        microseconds: c_uint,
    ) -> c_int {
        let time = match u64::from(microseconds).checked_mul(1_000) {
            Some(v) => Duration::from_nanos(v),
            None => Duration::MAX,
        };
        if let Some(co) = SchedulableCoroutine::current() {
            let syscall = Syscall::usleep;
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
        _ = EventLoops::wait_just(Some(time));
        reset_errno();
        0
    }
}
