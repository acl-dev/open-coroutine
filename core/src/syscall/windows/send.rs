use std::ffi::c_int;
use windows_sys::core::PCSTR;
use windows_sys::Win32::Networking::WinSock::{SEND_RECV_FLAGS, SOCKET};

trait SendSyscall {
    extern "system" fn send(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, PCSTR, c_int, SEND_RECV_FLAGS) -> c_int>,
        fd: SOCKET,
        buf: PCSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> c_int;
}

impl_syscall!(SendSyscallFacade, NioSendSyscall, RawSendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_facade!(SendSyscallFacade, SendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_nio_write_buf!(NioSendSyscall, SendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_raw!(RawSendSyscall, SendSyscall, windows_sys::Win32::Networking::WinSock,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);
