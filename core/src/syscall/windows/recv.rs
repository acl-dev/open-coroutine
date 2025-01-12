use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::core::PSTR;
use windows_sys::Win32::Networking::WinSock::{SEND_RECV_FLAGS, SOCKET};

#[must_use]
pub extern "system" fn recv(
    fn_ptr: Option<&extern "system" fn(SOCKET, PSTR, c_int, SEND_RECV_FLAGS) -> c_int>,
    fd: SOCKET,
    buf: PSTR,
    len: c_int,
    flags: SEND_RECV_FLAGS,
) -> c_int {
    // cfg_if::cfg_if! {
    //     if #[cfg(all(windows, feature = "iocp"))] {
    //         static CHAIN: Lazy<
    //             RecvSyscallFacade<IocpRecvSyscall<NioRecvSyscall<RawRecvSyscall>>>
    //         > = Lazy::new(Default::default);
    //     } else {
    //         static CHAIN: Lazy<RecvSyscallFacade<NioRecvSyscall<RawRecvSyscall>>> =
    //             Lazy::new(Default::default);
    //     }
    // }
    static CHAIN: Lazy<RecvSyscallFacade<NioRecvSyscall<RawRecvSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.recv(fn_ptr, fd, buf, len, flags)
}

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

impl_facade!(RecvSyscallFacade, RecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_iocp!(IocpRecvSyscall, RecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_nio_read_buf!(NioRecvSyscall, RecvSyscall,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);

impl_raw!(RawRecvSyscall, RecvSyscall, windows_sys::Win32::Networking::WinSock,
    recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int
);
