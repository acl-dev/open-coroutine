use once_cell::sync::Lazy;
use std::ffi::{c_char, c_int};

#[must_use]
pub extern "C" fn renameat(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_char, c_int, *const c_char) -> c_int>,
    olddirfd: c_int,
    oldpath: *const c_char,
    newdirfd: c_int,
    newpath: *const c_char,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<RenameatSyscallFacade<IoUringRenameatSyscall<RawRenameatSyscall>>> =
                Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<RenameatSyscallFacade<RawRenameatSyscall>> = Lazy::new(Default::default);
        }
    }
    CHAIN.renameat(fn_ptr, olddirfd, oldpath, newdirfd, newpath)
}

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

impl_facade!(RenameatSyscallFacade, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);

impl_io_uring!(IoUringRenameatSyscall, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);

impl_raw!(RawRenameatSyscall, RenameatSyscall,
    renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int
);
