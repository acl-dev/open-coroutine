use crate::net::EventLoops;
use std::ffi::c_int;

trait CloseSyscall {
    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int;
}

impl_syscall!(CloseSyscallFacade, IoUringCloseSyscall, NioCloseSyscall, RawCloseSyscall,
    close(fd: c_int) -> c_int
);

impl_facade!(CloseSyscallFacade, CloseSyscall, close(fd: c_int) -> c_int);

impl_io_uring!(IoUringCloseSyscall, CloseSyscall, close(fd: c_int) -> c_int);

#[repr(C)]
#[derive(Debug, Default)]
struct NioCloseSyscall<I: CloseSyscall> {
    inner: I,
}

impl<I: CloseSyscall> CloseSyscall for NioCloseSyscall<I> {
    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
        _ = EventLoops::del_event(fd);
        self.inner.close(fn_ptr, fd)
    }
}

impl_raw!(RawCloseSyscall, CloseSyscall, close(fd: c_int) -> c_int);
