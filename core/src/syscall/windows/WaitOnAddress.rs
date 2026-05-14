use std::ffi::{c_uint, c_void};
use std::time::Duration;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{ERROR_TIMEOUT, FALSE, TRUE};
use crate::common::{get_timeout_time, now};
use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};
use crate::syscall::reset_errno;
use crate::syscall::set_errno;

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

impl_syscall!(WaitOnAddressSyscallFacade, NioWaitOnAddressSyscall, RawWaitOnAddressSyscall,
    WaitOnAddress(
        address: *const c_void,
        compareaddress: *const c_void,
        addresssize: usize,
        dwmilliseconds: c_uint
    ) -> BOOL
);

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
        // Not inside a coroutine: call the real function directly.  This avoids turning
        // every internal runtime WaitOnAddress (parking_lot, once_cell, std::sync::Once)
        // into a slow 1 ms polling loop and prevents recursive EventLoops::wait_event
        // calls from corrupting coroutine scheduling state.
        if SchedulableCoroutine::current().is_none() {
            return self.inner.WaitOnAddress(
                fn_ptr, address, compareaddress, addresssize, dwmilliseconds,
            );
        }
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
            // Use suspender.until() directly instead of EventLoops::wait_event() to avoid
            // the recursion: EventLoops::wait_event → DashMap → parking_lot → WaitOnAddress
            // → NioWaitOnAddressSyscall → EventLoops::wait_event → …  which occurs on
            // nightly Windows where std internals call WaitOnAddress for mutex operations.
            // suspender.until() yields the coroutine without touching any DashMap or
            // parking_lot primitive, so there is no re-entrancy risk.
            if let Some(suspender) = SchedulableSuspender::current() {
                suspender.until(get_timeout_time(Duration::from_nanos(wait_time)));
            } else {
                // No suspender available (shouldn't happen inside a coroutine).
                // Fall back to the real WaitOnAddress with remaining time.
                return self.inner.WaitOnAddress(
                    fn_ptr,
                    address,
                    compareaddress,
                    addresssize,
                    (left_time / 1_000_000).try_into().unwrap_or(c_uint::MAX),
                );
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