use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::SOCKET;

trait ListenSyscall {
    extern "system" fn listen(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int) -> c_int>,
        fd: SOCKET,
        backlog: c_int,
    ) -> c_int;
}

impl_syscall!(ListenSyscallFacade, RawListenSyscall, listen(fd: SOCKET, backlog: c_int) -> c_int);

impl_facade!(ListenSyscallFacade, ListenSyscall, listen(fd: SOCKET, backlog: c_int) -> c_int);

impl_raw!(RawListenSyscall, ListenSyscall, windows_sys::Win32::Networking::WinSock,
    listen(fd: SOCKET, backlog: c_int) -> c_int
);
