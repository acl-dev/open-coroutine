use libc::{iovec, ssize_t};
use std::ffi::c_int;

trait WritevSyscall {
    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;
}

impl_syscall!(WritevSyscallFacade, IoUringWritevSyscall, NioWritevSyscall, RawWritevSyscall,
    writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_facade!(WritevSyscallFacade, WritevSyscall,
    writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_io_uring_write!(IoUringWritevSyscall, WritevSyscall,
    writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_nio_write_iovec!(NioWritevSyscall, WritevSyscall,
    writev(fd: c_int, iov: *const iovec, iovcnt: c_int,) -> ssize_t
);

impl_raw!(RawWritevSyscall, WritevSyscall,
    writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);
