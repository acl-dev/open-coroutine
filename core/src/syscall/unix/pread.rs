use libc::{off_t, size_t, ssize_t};
use std::ffi::{c_int, c_void};

trait PreadSyscall {
    extern "C" fn pread(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        offset: off_t,
    ) -> ssize_t;
}

impl_syscall!(PreadSyscallFacade, IoUringPreadSyscall, NioPreadSyscall, RawPreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_facade!(PreadSyscallFacade, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_io_uring_read!(IoUringPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_nio_read_buf!(NioPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_raw!(RawPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);
