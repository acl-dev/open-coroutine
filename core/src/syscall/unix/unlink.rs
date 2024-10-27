use std::ffi::{c_char, c_int};
use once_cell::sync::Lazy;

#[must_use]
pub extern "C" fn unlink(
    fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
    src: *const c_char,
) -> c_int{
    static CHAIN: Lazy<UnlinkSyscallFacade<RawUnlinkSyscall>> = Lazy::new(Default::default);
    CHAIN.unlink(fn_ptr, src)
}

trait LinkSyscall {
    extern "C" fn unlink(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char) -> c_int>,
        src: *const c_char
    ) -> c_int;
}

impl_facade!(UnlinkSyscallFacade, LinkSyscall,
    unlink(src: *const c_char) -> c_int
);

impl_raw!(RawUnlinkSyscall, LinkSyscall,
    unlink(src: *const c_char) -> c_int
);
