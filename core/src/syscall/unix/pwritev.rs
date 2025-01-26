use libc::{iovec, off_t, ssize_t};
use std::ffi::c_int;

trait PwritevSyscall {
    extern "C" fn pwritev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t;
}

impl_syscall!(PwritevSyscallFacade, IoUringPwritevSyscall, NioPwritevSyscall, RawPwritevSyscall,
    pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_facade!(PwritevSyscallFacade, PwritevSyscall,
    pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_io_uring_write!(IoUringPwritevSyscall, PwritevSyscall,
    pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_nio_write_iovec!(NioPwritevSyscall, PwritevSyscall,
    pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_raw!(RawPwritevSyscall, PwritevSyscall,
    pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);
