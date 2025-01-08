use std::ffi::{c_int, c_void};
use libc::{socklen_t, timeval};
use once_cell::sync::Lazy;
use crate::syscall::get_time_limit;
use crate::syscall::unix::{RECV_TIME_LIMIT, SEND_TIME_LIMIT};

#[must_use]
pub extern "C" fn setsockopt(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *const c_void, socklen_t) -> c_int>,
    socket: c_int,
    level: c_int,
    name: c_int,
    value: *const c_void,
    option_len: socklen_t
) -> c_int{
    static CHAIN: Lazy<SetsockoptSyscallFacade<NioSetsockoptSyscall<RawSetsockoptSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.setsockopt(fn_ptr, socket, level, name, value, option_len)
}

trait SetsockoptSyscall {
    extern "C" fn setsockopt(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *const c_void, socklen_t) -> c_int>,
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        option_len: socklen_t
    ) -> c_int;
}

impl_facade!(SetsockoptSyscallFacade, SetsockoptSyscall,
    setsockopt(socket: c_int, level: c_int, name: c_int, value: *const c_void, option_len: socklen_t) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioSetsockoptSyscall<I: SetsockoptSyscall> {
    inner: I,
}

impl<I: SetsockoptSyscall> SetsockoptSyscall for NioSetsockoptSyscall<I> {
    extern "C" fn setsockopt(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *const c_void, socklen_t) -> c_int>,
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *const c_void,
        option_len: socklen_t
    ) -> c_int {
        let r= self.inner.setsockopt(fn_ptr, socket, level, name, value, option_len);
        if 0 == r && libc::SOL_SOCKET == level {
            if libc::SO_SNDTIMEO == name {
                assert!(SEND_TIME_LIMIT.insert(socket, get_time_limit(unsafe { &*value.cast::<timeval>() })).is_none());
            } else if libc::SO_RCVTIMEO == name {
                assert!(RECV_TIME_LIMIT.insert(socket, get_time_limit(unsafe { &*value.cast::<timeval>() })).is_none());
            }
        }
        r
    }
}

impl_raw!(RawSetsockoptSyscall, SetsockoptSyscall,
    setsockopt(socket: c_int, level: c_int, name: c_int, value: *const c_void, option_len: socklen_t) -> c_int
);
