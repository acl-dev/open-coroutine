use std::ffi::{c_char, c_int};

trait LinkSyscall {
    extern "C" fn unlink(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
        src: *const c_char
    ) -> c_int;
}

impl_syscall!(UnlinkSyscallFacade, RawUnlinkSyscall,
    unlink(src: *const c_char) -> c_int
);

impl_facade!(UnlinkSyscallFacade, LinkSyscall,
    unlink(src: *const c_char) -> c_int
);

impl_raw!(RawUnlinkSyscall, LinkSyscall,
    unlink(src: *const c_char) -> c_int
);
