use libc::{iovec, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn writev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                WritevSyscallFacade<IoUringWritevSyscall<NioWritevSyscall<RawWritevSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<WritevSyscallFacade<NioWritevSyscall<RawWritevSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.writev(fn_ptr, fd, iov, iovcnt)
}

trait WritevSyscall {
    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;
}

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
