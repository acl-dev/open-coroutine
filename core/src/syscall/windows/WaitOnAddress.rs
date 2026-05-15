use std::ffi::{c_uint, c_void};
use windows_sys::core::BOOL;

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
        // Delegate directly to the real WaitOnAddress without any NIO polling loop.
        //
        // A NIO loop (poll every 1 ms, yield via EventLoops::wait_event for 10 ms) was
        // tried, but it caused two distinct problems on Windows:
        //
        // 1. Recursion: EventLoops::wait_event accesses DashMap/parking_lot internals which
        //    call WaitOnAddress, creating an infinite recursion chain that stack-overflows.
        //
        // 2. Excessive overhead: on nightly Windows, std uses WaitOnAddress for many
        //    internal mutex operations (Mutex, Condvar, Arc, channels …).  Each call from
        //    within a coroutine incurred an ~11 ms overhead, causing the socket_co_server
        //    integration test to exceed its 30 s timeout.
        //
        // Passing through directly avoids both issues.  Any WaitOnAddress call from within
        // a coroutine simply blocks the event-loop thread for its natural duration, which is
        // acceptable because the durations in practice are very short (µs range).
        self.inner.WaitOnAddress(fn_ptr, address, compareaddress, addresssize, dwmilliseconds)
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