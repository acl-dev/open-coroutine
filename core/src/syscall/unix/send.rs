use libc::{size_t, ssize_t};
use std::ffi::{c_int, c_void};

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

impl_syscall!(SendSyscallFacade, IoUringSendSyscall, NioSendSyscall, RawSendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_facade!(SendSyscallFacade, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_io_uring_write!(IoUringSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_nio_write_buf!(NioSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_raw!(RawSendSyscall, SendSyscall,
    send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t
);
