use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn listen(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    fd: c_int,
    backlog: c_int,
) -> c_int {
    static CHAIN: Lazy<ListenSyscallFacade<RawListenSyscall>> = Lazy::new(Default::default);
    CHAIN.listen(fn_ptr, fd, backlog)
}

trait ListenSyscall {
    extern "C" fn listen(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        fd: c_int,
        backlog: c_int,
    ) -> c_int;
}

impl_facade!(ListenSyscallFacade, ListenSyscall, listen(fd: c_int, backlog: c_int) -> c_int);

impl_raw!(RawListenSyscall, ListenSyscall, listen(fd: c_int, backlog: c_int) -> c_int);
