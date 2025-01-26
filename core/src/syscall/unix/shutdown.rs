use crate::net::EventLoops;
use crate::syscall::set_errno;
use std::ffi::c_int;

trait ShutdownSyscall {
    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        fd: c_int,
        how: c_int,
    ) -> c_int;
}

impl_syscall!(ShutdownSyscallFacade, IoUringShutdownSyscall, NioShutdownSyscall, RawShutdownSyscall,
    shutdown(fd: c_int, how: c_int) -> c_int
);

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
