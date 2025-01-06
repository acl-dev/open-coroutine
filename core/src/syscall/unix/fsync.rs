use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn fsync(
    fn_ptr: Option<&extern "C" fn(c_int) -> c_int>,
    fd: c_int,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<FsyncSyscallFacade<IoUringFsyncSyscall<RawFsyncSyscall>>> =
                Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<FsyncSyscallFacade<RawFsyncSyscall>> = Lazy::new(Default::default);
        }
    }
    CHAIN.fsync(fn_ptr, fd)
}

trait FsyncSyscall {
    extern "C" fn fsync(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int) -> c_int>,
        fd: c_int,
    ) -> c_int;
}

impl_facade!(FsyncSyscallFacade, FsyncSyscall, fsync(fd: c_int) -> c_int);

impl_io_uring!(IoUringFsyncSyscall, FsyncSyscall, fsync(fd: c_int) -> c_int);

impl_raw!(RawFsyncSyscall, FsyncSyscall, fsync(fd: c_int) -> c_int);
