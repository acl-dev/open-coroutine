use libc::{size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn recvfrom(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut c_void, size_t, c_int, *mut sockaddr, *mut socklen_t) -> ssize_t,
    >,
    fd: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> ssize_t {
    static CHAIN: Lazy<RecvfromSyscallFacade<NioRecvfromSyscall<RawRecvfromSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.recvfrom(fn_ptr, fd, buf, len, flags, addr, addrlen)
}

trait RecvfromSyscall {
    extern "C" fn recvfrom(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *mut c_void,
                size_t,
                c_int,
                *mut sockaddr,
                *mut socklen_t,
            ) -> ssize_t,
        >,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> ssize_t;
}

impl_facade!(RecvfromSyscallFacade, RecvfromSyscall,
    recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t
    ) -> ssize_t
);

impl_nio_read_buf!(NioRecvfromSyscall, RecvfromSyscall,
    recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t
    ) -> ssize_t
);

impl_raw!(RawRecvfromSyscall, RecvfromSyscall,
    recvfrom(
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t
    ) -> ssize_t
);
