use std::ffi::c_int;
use once_cell::sync::Lazy;
use windows_sys::core::PSTR;
use crate::syscall::windows::{RECV_TIME_LIMIT, SEND_TIME_LIMIT};
use windows_sys::Win32::Networking::WinSock::{SO_RCVTIMEO, SO_SNDTIMEO, SOCKET, SOL_SOCKET};

#[must_use]
pub extern "system" fn setsockopt(
    fn_ptr: Option<&extern "system" fn(SOCKET, c_int, c_int, PSTR, c_int) -> c_int>,
    socket: SOCKET,
    level: c_int,
    name: c_int,
    value: PSTR,
    option_len: c_int
) -> c_int {
    static CHAIN: Lazy<SetsockoptSyscallFacade<NioSetsockoptSyscall<RawSetsockoptSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.setsockopt(fn_ptr, socket, level, name, value, option_len)
}

trait SetsockoptSyscall {
    extern "system" fn setsockopt(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int, c_int, PSTR, c_int) -> c_int>,
        socket: SOCKET,
        level: c_int,
        name: c_int,
        value: PSTR,
        option_len: c_int
    ) -> c_int;
}

impl_facade!(SetsockoptSyscallFacade, SetsockoptSyscall,
    setsockopt(socket: SOCKET, level: c_int, name: c_int, value: PSTR, option_len: c_int) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioSetsockoptSyscall<I: SetsockoptSyscall> {
    inner: I,
}

#[allow(clippy::cast_ptr_alignment)]
impl<I: SetsockoptSyscall> SetsockoptSyscall for NioSetsockoptSyscall<I> {
    extern "system" fn setsockopt(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int, c_int, PSTR, c_int) -> c_int>,
        socket: SOCKET,
        level: c_int,
        name: c_int,
        value: PSTR,
        option_len: c_int
    ) -> c_int {
        let r= self.inner.setsockopt(fn_ptr, socket, level, name, value, option_len);
        if 0 == r && SOL_SOCKET == level {
            if SO_SNDTIMEO == name {
                let ms = unsafe { *value.cast::<c_int>() };
                let mut time_limit = u64::try_from(ms)
                    .expect("overflow")
                    .saturating_mul(1_000_000);
                if 0 == time_limit {
                    // 取消超时
                    time_limit = u64::MAX;
                }
                assert!(SEND_TIME_LIMIT.insert(socket, time_limit).is_none());
            } else if SO_RCVTIMEO == name {
                let ms = unsafe { *value.cast::<c_int>() };
                let mut time_limit = u64::try_from(ms)
                    .expect("overflow")
                    .saturating_mul(1_000_000);
                if 0 == time_limit {
                    // 取消超时
                    time_limit = u64::MAX;
                }
                assert!(RECV_TIME_LIMIT.insert(socket, time_limit).is_none());
            }
        }
        r
    }
}

impl_raw!(RawSetsockoptSyscall, SetsockoptSyscall, windows_sys::Win32::Networking::WinSock,
    setsockopt(socket: SOCKET, level: c_int, name: c_int, value: PSTR, option_len: c_int) -> c_int
);
