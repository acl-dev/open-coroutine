use std::ffi::{c_char, c_int};
use once_cell::sync::Lazy;

#[must_use]
pub extern "C" fn link(
    fn_ptr: Option<&extern "C" fn(*const c_char, *const c_char) -> c_int>,
    src: *const c_char,
    dst: *const c_char
) -> c_int{
    static CHAIN: Lazy<LinkSyscallFacade<RawLinkSyscall>> = Lazy::new(Default::default);
    CHAIN.link(fn_ptr, src, dst)
}

trait LinkSyscall {
    extern "C" fn link(
        &self,
        fn_ptr: Option<&extern "C" fn(*const c_char, *const c_char) -> c_int>,
        src: *const c_char,
        dst: *const c_char
    ) -> c_int;
}

impl_facade!(LinkSyscallFacade, LinkSyscall,
    link(src: *const c_char, dst: *const c_char) -> c_int
);

impl_raw!(RawLinkSyscall, LinkSyscall,
    link(src: *const c_char, dst: *const c_char) -> c_int
);
