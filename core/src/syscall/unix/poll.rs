use crate::net::EventLoops;
use libc::{nfds_t, pollfd};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use std::time::Duration;

#[must_use]
pub extern "C" fn poll(
    fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
    fds: *mut pollfd,
    nfds: nfds_t,
    timeout: c_int,
) -> c_int {
    static CHAIN: Lazy<PollSyscallFacade<NioPollSyscall<RawPollSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.poll(fn_ptr, fds, nfds, timeout)
}

trait PollSyscall {
    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int;
}

impl_facade!(PollSyscallFacade, PollSyscall,
    poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioPollSyscall<I: PollSyscall> {
    inner: I,
}

impl<I: PollSyscall> PollSyscall for NioPollSyscall<I> {
    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int {
        let mut t = if timeout < 0 { c_int::MAX } else { timeout };
        let mut x = 1;
        let mut r;
        // just check poll every x ms
        loop {
            r = self.inner.poll(fn_ptr, fds, nfds, 0);
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

impl_raw!(RawPollSyscall, PollSyscall,
    poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int
);
