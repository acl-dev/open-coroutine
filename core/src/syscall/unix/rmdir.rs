use std::ffi::{c_char, c_int};

trait RmdirSyscall {
    extern "C" fn rmdir(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
        path: *const c_char,
    ) -> c_int;
}

impl_syscall!(RmdirSyscallFacade, RawRmdirSyscall,
    rmdir(path: *const c_char) -> c_int
);

impl_facade!(RmdirSyscallFacade, RmdirSyscall,
    rmdir(path: *const c_char) -> c_int
);

impl_raw!(RawRmdirSyscall, RmdirSyscall,
    rmdir(path: *const c_char) -> c_int
);
