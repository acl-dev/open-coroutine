use crate::net::event_loop::EventLoops;
use once_cell::sync::Lazy;
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

#[derive(Debug, Default)]
struct SleepSyscallFacade<I: SleepSyscall> {
    inner: I,
}

impl<I: SleepSyscall> SleepSyscall for SleepSyscallFacade<I> {
    extern "system" fn Sleep(
        &self,
        fn_ptr: Option<&StaticDetour<unsafe extern "system" fn(u32)>>,
        dw_milliseconds: u32,
    ) {
        crate::info!("sleep hooked");
        self.inner.Sleep(fn_ptr, dw_milliseconds);
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct NioSleepSyscall {}

impl SleepSyscall for NioSleepSyscall {
    extern "system" fn Sleep(
        &self,
        _: Option<&StaticDetour<unsafe extern "system" fn(u32)>>,
        dw_milliseconds: u32,
    ) {
        _ = EventLoops::wait_just(Some(Duration::from_millis(u64::from(dw_milliseconds))));
    }
}
