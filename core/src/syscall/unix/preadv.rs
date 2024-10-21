use libc::{iovec, off_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn preadv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                PreadvSyscallFacade<IoUringPreadvSyscall<NioPreadvSyscall<RawPreadvSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<PreadvSyscallFacade<NioPreadvSyscall<RawPreadvSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.preadv(fn_ptr, fd, iov, iovcnt, offset)
}

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

impl_facade!(PreadvSyscallFacade, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_io_uring!(IoUringPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_nio_read_iovec!(NioPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);

impl_raw!(RawPreadvSyscall, PreadvSyscall,
    preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t
);
