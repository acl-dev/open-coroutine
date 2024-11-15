use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::core::PCSTR;
use windows_sys::Win32::Networking::WinSock::{SEND_RECV_FLAGS, SOCKET};

#[must_use]
pub extern "system" fn send(
    fn_ptr: Option<&extern "system" fn(SOCKET, PCSTR, c_int, SEND_RECV_FLAGS) -> c_int>,
    fd: SOCKET,
    buf: PCSTR,
    len: c_int,
    flags: SEND_RECV_FLAGS,
) -> c_int {
    // cfg_if::cfg_if! {
    //     if #[cfg(all(windows, feature = "iocp"))] {
    //         static CHAIN: Lazy<
    //             SendSyscallFacade<IocpSendSyscall<NioSendSyscall<RawSendSyscall>>>
    //         > = Lazy::new(Default::default);
    //     } else {
    //         static CHAIN: Lazy<SendSyscallFacade<NioSendSyscall<RawSendSyscall>>> =
    //             Lazy::new(Default::default);
    //     }
    // }
    static CHAIN: Lazy<SendSyscallFacade<NioSendSyscall<RawSendSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.send(fn_ptr, fd, buf, len, flags)
}

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

impl_facade!(SendSyscallFacade, SendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_iocp!(IocpSendSyscall, SendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_nio_write_buf!(NioSendSyscall, SendSyscall,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_raw!(RawSendSyscall, SendSyscall, windows_sys::Win32::Networking::WinSock,
    send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);
