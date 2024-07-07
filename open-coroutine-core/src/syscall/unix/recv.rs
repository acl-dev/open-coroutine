use libc::{size_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

#[must_use]
pub extern "C" fn recv(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                RecvSyscallFacade<IoUringRecvSyscall<NioRecvSyscall<RawRecvSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<RecvSyscallFacade<NioRecvSyscall<RawRecvSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.recv(fn_ptr, socket, buf, len, flags)
}

trait RecvSyscall {
    extern "C" fn recv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t;
}

impl_facade!(RecvSyscallFacade, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_io_uring!(IoUringRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_nio_expected_read!(NioRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);

impl_raw!(RawRecvSyscall, RecvSyscall,
    recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t
);
