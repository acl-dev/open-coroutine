use libc::{size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn send(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                SendSyscallFacade<IoUringSendSyscall<NioSendSyscall<RawSendSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<SendSyscallFacade<NioSendSyscall<RawSendSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.send(fn_ptr, fd, buf, len, flags)
}

trait SendSyscall {
    extern "C" fn send(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t;
}

impl_facade!(SendSyscallFacade, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_io_uring!(IoUringSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_nio_expected_write!(NioSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_raw!(RawSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);
