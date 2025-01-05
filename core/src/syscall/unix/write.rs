use libc::{size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn write(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    len: size_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                WriteSyscallFacade<IoUringWriteSyscall<NioWriteSyscall<RawWriteSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<WriteSyscallFacade<NioWriteSyscall<RawWriteSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.write(fn_ptr, fd, buf, len)
}

trait WriteSyscall {
    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
    ) -> ssize_t;
}

impl_facade!(WriteSyscallFacade, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_io_uring_write!(IoUringWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_nio_write_buf!(NioWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_raw!(RawWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);
