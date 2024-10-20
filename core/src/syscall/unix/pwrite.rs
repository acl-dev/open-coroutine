use libc::{off_t, size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn pwrite(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                PwriteSyscallFacade<IoUringPwriteSyscall<NioPwriteSyscall<RawPwriteSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<PwriteSyscallFacade<NioPwriteSyscall<RawPwriteSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.pwrite(fn_ptr, fd, buf, count, offset)
}

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

impl_facade!(PwriteSyscallFacade, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_io_uring!(IoUringPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_nio_write_buf!(NioPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);

impl_raw!(RawPwriteSyscall, PwriteSyscall,
    pwrite(fd: c_int, buf: *const c_void, len: size_t, offset: off_t) -> ssize_t
);
