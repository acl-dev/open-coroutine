use libc::{size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn sendto(
    fn_ptr: Option<
        &extern "C" fn(c_int, *const c_void, size_t, c_int, *const sockaddr, socklen_t) -> ssize_t,
    >,
    fd: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                SendtoSyscallFacade<IoUringSendtoSyscall<NioSendtoSyscall<RawSendtoSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<SendtoSyscallFacade<NioSendtoSyscall<RawSendtoSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.sendto(fn_ptr, fd, buf, len, flags, addr, addrlen)
}

trait SendtoSyscall {
    extern "C" fn sendto(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *const c_void,
                size_t,
                c_int,
                *const sockaddr,
                socklen_t,
            ) -> ssize_t,
        >,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> ssize_t;
}

impl_facade!(SendtoSyscallFacade, SendtoSyscall,
    sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int,
        addr: *const sockaddr, addrlen: socklen_t) -> ssize_t
);

impl_io_uring!(IoUringSendtoSyscall, SendtoSyscall,
    sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int,
        addr: *const sockaddr, addrlen: socklen_t) -> ssize_t
);

impl_nio_write_buf!(NioSendtoSyscall, SendtoSyscall,
    sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int,
        addr: *const sockaddr, addrlen: socklen_t) -> ssize_t
);

impl_raw!(RawSendtoSyscall, SendtoSyscall,
    sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int,
        addr: *const sockaddr, addrlen: socklen_t) -> ssize_t
);
