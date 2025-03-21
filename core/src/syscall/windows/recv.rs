use std::ffi::c_int;
use windows_sys::core::PSTR;
use windows_sys::Win32::Networking::WinSock::{SEND_RECV_FLAGS, SOCKET};

trait RecvSyscall {
    extern "system" fn recv(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, PSTR, c_int, SEND_RECV_FLAGS) -> c_int>,
        fd: SOCKET,
        buf: PSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> c_int;
}

impl_syscall!(RecvSyscallFacade, NioRecvSyscall, RawRecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_facade!(RecvSyscallFacade, RecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_nio_read_buf!(NioRecvSyscall, RecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_raw!(RawRecvSyscall, RecvSyscall, windows_sys::Win32::Networking::WinSock,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);
