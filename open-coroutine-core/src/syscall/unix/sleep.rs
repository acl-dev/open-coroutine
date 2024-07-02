use crate::common::{Current, Named};
use crate::constants::{Syscall, SyscallState};
use crate::net::event_loop::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use crate::syscall::common::reset_errno;
use once_cell::sync::Lazy;
use open_coroutine_timer::get_timeout_time;
use std::ffi::c_uint;
use std::time::Duration;

#[must_use]
pub extern "C" fn sleep(fn_ptr: Option<&extern "C" fn(c_uint) -> c_uint>, secs: c_uint) -> c_uint {
    static CHAIN: Lazy<SleepSyscallFacade<NioSleepSyscall>> = Lazy::new(Default::default);
    CHAIN.sleep(fn_ptr, secs)
}

trait SleepSyscall {
    extern "C" fn sleep(
        &self,
        fn_ptr: Option<&extern "C" fn(c_uint) -> c_uint>,
        secs: c_uint,
    ) -> c_uint;
}

impl_facade!(SleepSyscallFacade, SleepSyscall, sleep(secs: c_uint) -> c_uint);

#[derive(Debug, Copy, Clone, Default)]
struct NioSleepSyscall {}

impl SleepSyscall for NioSleepSyscall {
    extern "C" fn sleep(
        &self,
        _: Option<&extern "C" fn(c_uint) -> c_uint>,
        secs: c_uint,
    ) -> c_uint {
        let time = Duration::from_secs(u64::from(secs));
        if let Some(co) = SchedulableCoroutine::current() {
            let syscall = Syscall::sleep;
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
