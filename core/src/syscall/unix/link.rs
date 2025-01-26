use std::ffi::{c_char, c_int};

trait LinkSyscall {
    extern "C" fn link(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char, *const c_char) -> c_int>,
        src: *const c_char,
        dst: *const c_char
    ) -> c_int;
}

impl_syscall!(LinkSyscallFacade, RawLinkSyscall,
    link(src: *const c_char, dst: *const c_char) -> c_int
);

impl_facade!(LinkSyscallFacade, LinkSyscall,
    link(src: *const c_char, dst: *const c_char) -> c_int
);

impl_raw!(RawLinkSyscall, LinkSyscall,
    link(src: *const c_char, dst: *const c_char) -> c_int
);
