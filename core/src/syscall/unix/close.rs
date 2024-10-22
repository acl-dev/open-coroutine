use crate::net::EventLoops;
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn close(fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                CloseSyscallFacade<IoUringCloseSyscall<NioCloseSyscall<RawCloseSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<CloseSyscallFacade<NioCloseSyscall<RawCloseSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.close(fn_ptr, fd)
}

trait CloseSyscall {
    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int;
}

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
