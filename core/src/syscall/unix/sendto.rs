use libc::{size_t, sockaddr, socklen_t, ssize_t};
use std::ffi::{c_int, c_void};

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

impl_syscall!(SendtoSyscallFacade, IoUringSendtoSyscall, NioSendtoSyscall, RawSendtoSyscall,
    sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t
    ) -> ssize_t
);

impl_facade!(SendtoSyscallFacade, SendtoSyscall,
    sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t
    ) -> ssize_t
);

impl_io_uring_write!(IoUringSendtoSyscall, SendtoSyscall,
    sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t
    ) -> ssize_t
);

impl_nio_write_buf!(NioSendtoSyscall, SendtoSyscall,
    sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t
    ) -> ssize_t
);

impl_raw!(RawSendtoSyscall, SendtoSyscall,
    sendto(
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t
    ) -> ssize_t
);
