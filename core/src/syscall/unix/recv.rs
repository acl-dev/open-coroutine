use libc::{size_t, ssize_t};
use std::ffi::{c_int, c_void};

trait RecvSyscall {
    extern "C" fn recv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t;
}

impl_syscall!(RecvSyscallFacade, IoUringRecvSyscall, NioRecvSyscall, RawRecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_facade!(RecvSyscallFacade, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_io_uring_read!(IoUringRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_nio_read_buf!(NioRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_raw!(RawRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);
