use libc::{off_t, size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn pread(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    len: size_t,
    offset: off_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                PreadSyscallFacade<IoUringPreadSyscall<NioPreadSyscall<RawPreadSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<PreadSyscallFacade<NioPreadSyscall<RawPreadSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.pread(fn_ptr, fd, buf, len, offset)
}

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

impl_facade!(PreadSyscallFacade, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_io_uring!(IoUringPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_nio_read_buf!(NioPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_raw!(RawPreadSyscall, PreadSyscall,
    pread(fd: c_int, buf: *mut c_void, len: size_t, offset: off_t) -> ssize_t
);
