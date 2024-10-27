use libc::off_t;
use std::ffi::c_int;
use once_cell::sync::Lazy;

#[must_use]
pub extern "C" fn lseek(
    fn_ptr: Option<&extern "C" fn(c_int, off_t, c_int) -> off_t>,
    fd: c_int,
    offset: off_t,
    whence: c_int,
) -> off_t{
    static CHAIN: Lazy<LseekSyscallFacade<RawLseekSyscall>> = Lazy::new(Default::default);
    CHAIN.lseek(fn_ptr, fd, offset,whence)
}

trait LseekSyscall {
    extern "C" fn lseek(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, off_t, c_int) -> off_t>,
        fd: c_int,
        offset: off_t,
        whence: c_int,
    ) -> off_t;
}

impl_facade!(LseekSyscallFacade, LseekSyscall,
    lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t
);

impl_raw!(RawLseekSyscall, LseekSyscall,
    lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t
);
