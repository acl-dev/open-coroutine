use once_cell::sync::Lazy;
use std::ffi::{c_char, c_int};

#[must_use]
pub extern "C" fn rmdir(
    fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
    path: *const c_char,
) -> c_int {
    static CHAIN: Lazy<RmdirSyscallFacade<RawRmdirSyscall>> = Lazy::new(Default::default);
    CHAIN.rmdir(fn_ptr, path)
}

trait RmdirSyscall {
    extern "C" fn rmdir(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
        path: *const c_char,
    ) -> c_int;
}

impl_facade!(RmdirSyscallFacade, RmdirSyscall,
    rmdir(path: *const c_char) -> c_int
);

impl_raw!(RawRmdirSyscall, RmdirSyscall,
    rmdir(path: *const c_char) -> c_int
);
