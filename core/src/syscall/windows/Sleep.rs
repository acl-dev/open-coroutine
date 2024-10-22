use crate::net::EventLoops;
use once_cell::sync::Lazy;
use std::time::Duration;

pub extern "system" fn Sleep(fn_ptr: Option<&extern "system" fn(u32)>, dw_milliseconds: u32) {
    static CHAIN: Lazy<SleepSyscallFacade<NioSleepSyscall>> = Lazy::new(Default::default);
    CHAIN.Sleep(fn_ptr, dw_milliseconds);
}

trait SleepSyscall {
    extern "system" fn Sleep(&self, fn_ptr: Option<&extern "system" fn(u32)>, dw_milliseconds: u32);
}

impl_facade!(SleepSyscallFacade, SleepSyscall,
    Sleep(dw_milliseconds: u32) -> ()
);

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
struct NioSleepSyscall {}

impl SleepSyscall for NioSleepSyscall {
    extern "system" fn Sleep(&self, _: Option<&extern "system" fn(u32)>, dw_milliseconds: u32) {
        let time = Duration::from_millis(u64::from(dw_milliseconds));
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            let syscall = crate::common::constants::Syscall::Sleep;
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
        _ = EventLoops::wait_event(Some(time));
    }
}
