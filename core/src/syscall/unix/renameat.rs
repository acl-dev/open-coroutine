use std::ffi::{c_char, c_int};

trait RenameatSyscall {
    extern "C" fn renameat(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_char, c_int, *const c_char) -> c_int>,
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
    ) -> c_int;
}

impl_syscall2!(RenameatSyscallFacade, IoUringRenameatSyscall, RawRenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);

impl_facade!(RenameatSyscallFacade, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);

impl_io_uring!(IoUringRenameatSyscall, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);

impl_raw!(RawRenameatSyscall, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);
