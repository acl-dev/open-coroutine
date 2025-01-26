use libc::{iovec, off_t, ssize_t};
use std::ffi::c_int;

trait PreadvSyscall {
    extern "C" fn preadv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t;
}

impl_syscall!(PreadvSyscallFacade, IoUringPreadvSyscall, NioPreadvSyscall, RawPreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_facade!(PreadvSyscallFacade, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_io_uring_read!(IoUringPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_nio_read_iovec!(NioPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_raw!(RawPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);
