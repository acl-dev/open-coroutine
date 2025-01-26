use std::ffi::c_int;

trait FsyncSyscall {
    extern "C" fn fsync(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int) -> c_int>,
        fd: c_int,
    ) -> c_int;
}

impl_syscall2!(FsyncSyscallFacade, IoUringFsyncSyscall, RawFsyncSyscall, fsync(fd: c_int) -> c_int);

impl_facade!(FsyncSyscallFacade, FsyncSyscall, fsync(fd: c_int) -> c_int);

impl_io_uring!(IoUringFsyncSyscall, FsyncSyscall, fsync(fd: c_int) -> c_int);

impl_raw!(RawFsyncSyscall, FsyncSyscall, fsync(fd: c_int) -> c_int);
