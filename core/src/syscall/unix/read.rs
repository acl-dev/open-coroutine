use libc::{size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn read(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    len: size_t,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                ReadSyscallFacade<IoUringReadSyscall<NioReadSyscall<RawReadSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<ReadSyscallFacade<NioReadSyscall<RawReadSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.read(fn_ptr, fd, buf, len)
}

trait ReadSyscall {
    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
    ) -> ssize_t;
}

impl_facade!(ReadSyscallFacade, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_io_uring_read!(IoUringReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_nio_read_buf!(NioReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);

impl_raw!(RawReadSyscall, ReadSyscall,
    read(fd: c_int, buf: *mut c_void, len: size_t) -> ssize_t
);
