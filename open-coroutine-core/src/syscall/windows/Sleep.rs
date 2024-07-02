use crate::common::{Current, Named};
use crate::constants::{Syscall, SyscallState};
use crate::net::event_loop::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use once_cell::sync::Lazy;
use open_coroutine_timer::get_timeout_time;
use retour::StaticDetour;
use std::time::Duration;

pub extern "C" fn Sleep(
    fn_ptr: Option<&StaticDetour<unsafe extern "system" fn(u32)>>,
    dw_milliseconds: u32,
) {
    static CHAIN: Lazy<SleepSyscallFacade<NioSleepSyscall>> = Lazy::new(Default::default);
    CHAIN.Sleep(fn_ptr, dw_milliseconds);
}

trait SleepSyscall {
    extern "system" fn Sleep(
        &self,
        fn_ptr: Option<&StaticDetour<unsafe extern "system" fn(u32)>>,
        dw_milliseconds: u32,
    );
}

impl_facade!(SleepSyscallFacade, SleepSyscall,
    Sleep(dw_milliseconds: u32) -> ()
);

#[derive(Debug, Copy, Clone, Default)]
struct NioSleepSyscall {}

impl SleepSyscall for NioSleepSyscall {
    extern "system" fn Sleep(
        &self,
        _: Option<&StaticDetour<unsafe extern "system" fn(u32)>>,
        dw_milliseconds: u32,
    ) {
        let time = Duration::from_millis(u64::from(dw_milliseconds));
        if let Some(co) = SchedulableCoroutine::current() {
            let syscall = Syscall::Sleep;
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
    }
}
