use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn listen(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    backlog: c_int,
) -> c_int {
    static CHAIN: Lazy<ListenSyscallFacade<RawListenSyscall>> = Lazy::new(Default::default);
    CHAIN.listen(fn_ptr, socket, backlog)
}

trait ListenSyscall {
    extern "C" fn listen(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        backlog: c_int,
    ) -> c_int;
}

impl_facade!(ListenSyscallFacade, ListenSyscall, listen(socket: c_int, backlog: c_int) -> c_int);

impl_raw!(RawListenSyscall, ListenSyscall, listen(socket: c_int, backlog: c_int) -> c_int);
