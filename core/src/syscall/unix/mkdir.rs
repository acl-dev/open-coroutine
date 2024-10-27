use libc::mode_t;
use once_cell::sync::Lazy;
use std::ffi::{c_char, c_int};

#[must_use]
pub extern "C" fn mkdir(
    fn_ptr: Option<&extern "C" fn(*const c_char, mode_t) -> c_int>,
    path: *const c_char,
    mode: mode_t,
) -> c_int {
    static CHAIN: Lazy<MkdirSyscallFacade<RawMkdirSyscall>> = Lazy::new(Default::default);
    CHAIN.mkdir(fn_ptr, path, mode)
}

trait MkdirSyscall {
    extern "C" fn mkdir(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char, mode_t) -> c_int>,
        path: *const c_char,
        mode: mode_t,
    ) -> c_int;
}

impl_facade!(MkdirSyscallFacade, MkdirSyscall,
    mkdir(path: *const c_char, mode: mode_t) -> c_int
);

impl_raw!(RawMkdirSyscall, MkdirSyscall,
    mkdir(path: *const c_char, mode: mode_t) -> c_int
);
