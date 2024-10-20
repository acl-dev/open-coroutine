use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::SOCKET;

pub extern "system" fn listen(
    fn_ptr: Option<&extern "system" fn(SOCKET, c_int) -> c_int>,
    fd: SOCKET,
    backlog: c_int,
) -> c_int {
    static CHAIN: Lazy<ListenSyscallFacade<RawListenSyscall>> = Lazy::new(Default::default);
    CHAIN.listen(fn_ptr, fd, backlog)
}

trait ListenSyscall {
    extern "system" fn listen(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int) -> c_int>,
        fd: SOCKET,
        backlog: c_int,
    ) -> c_int;
}

impl_facade!(ListenSyscallFacade, ListenSyscall, listen(fd: SOCKET, backlog: c_int) -> c_int);

impl_raw!(RawListenSyscall, ListenSyscall, windows_sys::Win32::Networking::WinSock,
    listen(fd: SOCKET, backlog: c_int) -> c_int
);
