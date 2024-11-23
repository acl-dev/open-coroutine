use std::ffi::{c_uint, c_void};
use std::time::Duration;
use once_cell::sync::Lazy;
use windows_sys::Win32::Foundation::{BOOL, ERROR_TIMEOUT, FALSE, TRUE};
use crate::common::{get_timeout_time, now};
use crate::net::EventLoops;
use crate::syscall::reset_errno;
use crate::syscall::set_errno;

#[must_use]
pub extern "system" fn WaitOnAddress(
    fn_ptr: Option<&extern "system" fn(*const c_void, *const c_void, usize, c_uint) -> BOOL>,
    address: *const c_void,
    compareaddress: *const c_void,
    addresssize: usize,
    dwmilliseconds: c_uint
) -> BOOL {
    static CHAIN: Lazy<WaitOnAddressSyscallFacade<NioWaitOnAddressSyscall<RawWaitOnAddressSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.WaitOnAddress(fn_ptr, address, compareaddress, addresssize, dwmilliseconds)
}

trait WaitOnAddressSyscall {
    extern "system" fn WaitOnAddress(
        &self,
        fn_ptr: Option<&extern "system" fn(*const c_void, *const c_void, usize, c_uint) -> BOOL>,
        address: *const c_void,
        compareaddress: *const c_void,
        addresssize: usize,
        dwmilliseconds: c_uint
    ) -> BOOL;
}

impl_facade!(WaitOnAddressSyscallFacade, WaitOnAddressSyscall,
    WaitOnAddress(
        address: *const c_void,
        compareaddress: *const c_void,
        addresssize: usize,
        dwmilliseconds: c_uint
    ) -> BOOL
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioWaitOnAddressSyscall<I: WaitOnAddressSyscall> {
    inner: I,
}

impl<I: WaitOnAddressSyscall> WaitOnAddressSyscall for NioWaitOnAddressSyscall<I> {
    extern "system" fn WaitOnAddress(
        &self,
        fn_ptr: Option<&extern "system" fn(*const c_void, *const c_void, usize, c_uint) -> BOOL>,
        address: *const c_void,
        compareaddress: *const c_void,
        addresssize: usize,
        dwmilliseconds: c_uint
    ) -> BOOL {
        let timeout = get_timeout_time(Duration::from_millis(dwmilliseconds.into()));
        loop {
            let mut left_time = timeout.saturating_sub(now());
            if 0 == left_time {
                set_errno(ERROR_TIMEOUT);
                return FALSE;
            }
            let r = self.inner.WaitOnAddress(
                fn_ptr,
                address,
                compareaddress,
                addresssize,
                (left_time / 1_000_000).min(1).try_into().expect("overflow"),
            );
            if TRUE == r {
                reset_errno();
                return r;
            }
            left_time = timeout.saturating_sub(now());
            if 0 == left_time {
                set_errno(ERROR_TIMEOUT);
                return FALSE;
            }
            let wait_time = if left_time > 10_000_000 {
                10_000_000
            } else {
                left_time
            };
            if EventLoops::wait_event(Some(Duration::new(
                wait_time / 1_000_000_000,
                (wait_time % 1_000_000_000) as _,
            )))
                .is_err()
            {
                return r;
            }
        }
    }
}

impl_raw!(RawWaitOnAddressSyscall, WaitOnAddressSyscall, windows_sys::Win32::System::Threading,
    WaitOnAddress(
        address: *const c_void,
        compareaddress: *const c_void,
        addresssize: usize,
        dwmilliseconds: c_uint
    ) -> BOOL
);