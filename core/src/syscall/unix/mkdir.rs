use libc::mode_t;
use std::ffi::{c_char, c_int};

trait MkdirSyscall {
    extern "C" fn mkdir(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char, mode_t) -> c_int>,
        path: *const c_char,
        mode: mode_t,
    ) -> c_int;
}

impl_syscall!(MkdirSyscallFacade, RawMkdirSyscall,
    mkdir(path: *const c_char, mode: mode_t) -> c_int
);

impl_facade!(MkdirSyscallFacade, MkdirSyscall,
    mkdir(path: *const c_char, mode: mode_t) -> c_int
);

impl_raw!(RawMkdirSyscall, MkdirSyscall,
    mkdir(path: *const c_char, mode: mode_t) -> c_int
);
