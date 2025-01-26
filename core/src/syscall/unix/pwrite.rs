use libc::{off_t, size_t, ssize_t};
use std::ffi::{c_int, c_void};

trait PwriteSyscall {
    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t;
}

impl_syscall!(PwriteSyscallFacade, IoUringPwriteSyscall, NioPwriteSyscall, RawPwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_facade!(PwriteSyscallFacade, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_io_uring_write!(IoUringPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_nio_write_buf!(NioPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_raw!(RawPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);
