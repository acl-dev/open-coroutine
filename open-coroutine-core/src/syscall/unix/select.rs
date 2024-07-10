use crate::net::event_loop::EventLoops;
use libc::{fd_set, timeval};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use std::time::Duration;

#[must_use]
pub extern "C" fn select(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
    >,
    nfds: c_int,
    readfds: *mut fd_set,
    writefds: *mut fd_set,
    errorfds: *mut fd_set,
    timeout: *mut timeval,
) -> c_int {
    static CHAIN: Lazy<SelectSyscallFacade<NioSelectSyscall<RawSelectSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.select(fn_ptr, nfds, readfds, writefds, errorfds, timeout)
}

trait SelectSyscall {
    extern "C" fn select(
        &self,
        fn_ptr: Option<
            &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
        >,
        nfds: c_int,
        readfds: *mut fd_set,
        writefds: *mut fd_set,
        errorfds: *mut fd_set,
        timeout: *mut timeval,
    ) -> c_int;
}

impl_facade!(SelectSyscallFacade, SelectSyscall,
    select(nfds: c_int, readfds: *mut fd_set, writefds: *mut fd_set,
        errorfds: *mut fd_set, timeout: *mut timeval) -> c_int
);

#[derive(Debug, Default)]
struct NioSelectSyscall<I: SelectSyscall> {
    inner: I,
}

impl<I: SelectSyscall> SelectSyscall for NioSelectSyscall<I> {
    extern "C" fn select(
        &self,
        fn_ptr: Option<
            &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
        >,
        nfds: c_int,
        readfds: *mut fd_set,
        writefds: *mut fd_set,
        errorfds: *mut fd_set,
        timeout: *mut timeval,
    ) -> c_int {
        let mut t = if timeout.is_null() {
            c_uint::MAX
        } else {
            unsafe { ((*timeout).tv_sec as c_uint) * 1_000_000 + (*timeout).tv_usec as c_uint }
        };
        let mut o = timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        let mut s: [fd_set; 3] = unsafe { std::mem::zeroed() };
        if !readfds.is_null() {
            s[0] = unsafe { *readfds };
        }
        if !writefds.is_null() {
            s[1] = unsafe { *writefds };
        }
        if !errorfds.is_null() {
            s[2] = unsafe { *errorfds };
        }
        let mut x = 1;
        let mut r;
        // just check poll every x ms
        loop {
            r = self
                .inner
                .select(fn_ptr, nfds, readfds, writefds, errorfds, &mut o);
            if r != 0 || t == 0 {
                break;
            }
            _ = EventLoops::wait_just(Some(Duration::from_millis(u64::from(t.min(x)))));
            if t != c_uint::MAX {
                t = if t > x { t - x } else { 0 };
            }
            if x < 16 {
                x <<= 1;
            }

            if !readfds.is_null() {
                unsafe { *readfds = s[0] };
            }
            if !writefds.is_null() {
                unsafe { *writefds = s[1] };
            }
            if !errorfds.is_null() {
                unsafe { *errorfds = s[2] };
            }
            o.tv_sec = 0;
            o.tv_usec = 0;
        }
        r
    }
}

impl_raw!(RawSelectSyscall, SelectSyscall,
    select(nfds: c_int, readfds: *mut fd_set, writefds: *mut fd_set,
        errorfds: *mut fd_set, timeout: *mut timeval) -> c_int
);
