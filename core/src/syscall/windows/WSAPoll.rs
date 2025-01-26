use std::convert::TryInto;
use crate::net::EventLoops;
use std::ffi::{c_int, c_uint};
use std::time::Duration;
use windows_sys::Win32::Networking::WinSock::WSAPOLLFD;

trait PollSyscall {
    extern "system" fn WSAPoll(
        &self,
        fn_ptr: Option<&extern "system" fn(*mut WSAPOLLFD, c_uint, c_int) -> c_int>,
        fds: *mut WSAPOLLFD,
        nfds: c_uint,
        timeout: c_int,
    ) -> c_int;
}

impl_syscall!(PollSyscallFacade, NioPollSyscall, RawPollSyscall,
    WSAPoll(fds: *mut WSAPOLLFD, nfds: c_uint, timeout: c_int) -> c_int
);

impl_facade!(PollSyscallFacade, PollSyscall,
    WSAPoll(fds: *mut WSAPOLLFD, nfds: c_uint, timeout: c_int) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioPollSyscall<I: PollSyscall> {
    inner: I,
}

impl<I: PollSyscall> PollSyscall for NioPollSyscall<I> {
    extern "system" fn WSAPoll(
        &self,
        fn_ptr: Option<&extern "system" fn(*mut WSAPOLLFD, c_uint, c_int) -> c_int>,
        fds: *mut WSAPOLLFD,
        nfds: c_uint,
        timeout: c_int,
    ) -> c_int {
        let mut t = if timeout < 0 { c_int::MAX } else { timeout };
        let mut x = 1;
        let mut r;
        // just check poll every x ms
        loop {
            r = self.inner.WSAPoll(fn_ptr, fds, nfds, 0);
            if r != 0 || t == 0 {
                break;
            }
            _ = EventLoops::wait_event(Some(Duration::from_millis(t.min(x).try_into().expect("overflow"))));
            if t != c_int::MAX {
                t = if t > x { t - x } else { 0 };
            }
            if x < 16 {
                x <<= 1;
            }
        }
        r
    }
}

impl_raw!(RawPollSyscall, PollSyscall, windows_sys::Win32::Networking::WinSock,
    WSAPoll(fds: *mut WSAPOLLFD, nfds: c_uint, timeout: c_int) -> c_int
);
