use std::ffi::{c_char, c_int, c_uint};

trait Renameat2Syscall {
    extern "C" fn renameat2(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_char, c_int, *const c_char, c_uint) -> c_int>,
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int;
}

impl_syscall2!(Renameat2SyscallFacade, IoUringRenameat2Syscall, RawRenameat2Syscall,
    renameat2(
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int
);

impl_facade!(Renameat2SyscallFacade, Renameat2Syscall,
    renameat2(
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int
);

impl_io_uring!(IoUringRenameat2Syscall, Renameat2Syscall,
    renameat2(
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int
);

impl_raw!(RawRenameat2Syscall, Renameat2Syscall,
    renameat2(
        olddirfd: c_int,
        oldpath: *const c_char,
        newdirfd: c_int,
        newpath: *const c_char,
        flags: c_uint,
    ) -> c_int
);
