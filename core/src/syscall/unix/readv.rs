use libc::{iovec, ssize_t};
use std::ffi::c_int;

trait ReadvSyscall {
    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;
}

impl_syscall!(ReadvSyscallFacade, IoUringReadvSyscall, NioReadvSyscall, RawReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_facade!(ReadvSyscallFacade, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_io_uring_read!(IoUringReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_nio_read_iovec!(NioReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int,) -> ssize_t
);

impl_raw!(RawReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);
