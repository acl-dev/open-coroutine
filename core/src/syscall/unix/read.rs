use libc::{size_t, ssize_t};
use std::ffi::{c_int, c_void};

trait ReadSyscall {
    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
    ) -> ssize_t;
}

impl_syscall!(ReadSyscallFacade, IoUringReadSyscall, NioReadSyscall, RawReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_facade!(ReadSyscallFacade, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_io_uring_read!(IoUringReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_nio_read_buf!(NioReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_raw!(RawReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);
