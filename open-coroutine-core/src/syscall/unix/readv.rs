use libc::{iovec, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn readv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                ReadvSyscallFacade<IoUringReadvSyscall<NioReadvSyscall<RawReadvSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<ReadvSyscallFacade<NioReadvSyscall<RawReadvSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.readv(fn_ptr, fd, iov, iovcnt)
}

trait ReadvSyscall {
    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;
}

impl_facade!(ReadvSyscallFacade, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_io_uring!(IoUringReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);

impl_nio_read_iovec!(NioReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int,) -> ssize_t
);

impl_raw!(RawReadvSyscall, ReadvSyscall,
    readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t
);
