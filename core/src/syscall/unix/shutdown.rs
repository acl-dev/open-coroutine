use crate::net::EventLoops;
use crate::syscall::common::set_errno;
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn shutdown(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    how: c_int,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                ShutdownSyscallFacade<IoUringShutdownSyscall<NioShutdownSyscall<RawShutdownSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<ShutdownSyscallFacade<NioShutdownSyscall<RawShutdownSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.shutdown(fn_ptr, socket, how)
}

trait ShutdownSyscall {
    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        fd: c_int,
        how: c_int,
    ) -> c_int;
}

impl_facade!(ShutdownSyscallFacade, ShutdownSyscall, shutdown(fd: c_int, how: c_int) -> c_int);

impl_io_uring!(IoUringShutdownSyscall, ShutdownSyscall, shutdown(fd: c_int, how: c_int) -> c_int);

#[repr(C)]
#[derive(Debug, Default)]
struct NioShutdownSyscall<I: ShutdownSyscall> {
    inner: I,
}

impl<I: ShutdownSyscall> ShutdownSyscall for NioShutdownSyscall<I> {
    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        fd: c_int,
        how: c_int,
    ) -> c_int {
        _ = match how {
            libc::SHUT_RD => EventLoops::del_read_event(fd),
            libc::SHUT_WR => EventLoops::del_write_event(fd),
            libc::SHUT_RDWR => EventLoops::del_event(fd),
            _ => {
                set_errno(libc::EINVAL);
                return -1;
            }
        };
        self.inner.shutdown(fn_ptr, fd, how)
    }
}

impl_raw!(RawShutdownSyscall, ShutdownSyscall, shutdown(fd: c_int, how: c_int) -> c_int);
