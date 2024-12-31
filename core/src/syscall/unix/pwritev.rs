use libc::{iovec, off_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn pwritev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                PwritevSyscallFacade<IoUringPwritevSyscall<NioPwritevSyscall<RawPwritevSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<PwritevSyscallFacade<NioPwritevSyscall<RawPwritevSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.pwritev(fn_ptr, fd, iov, iovcnt, offset)
}

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
