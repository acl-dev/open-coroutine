use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};

#[must_use]
pub extern "system" fn accept(
    fn_ptr: Option<&extern "system" fn(SOCKET, *mut SOCKADDR, *mut c_int) -> SOCKET>,
    fd: SOCKET,
    address: *mut SOCKADDR,
    address_len: *mut c_int,
) -> SOCKET {
    // cfg_if::cfg_if! {
    //     if #[cfg(feature = "iocp")] {
    //         static CHAIN: Lazy<
    //             AcceptSyscallFacade<IocpAcceptSyscall<NioAcceptSyscall<RawAcceptSyscall>>>
    //         > = Lazy::new(Default::default);
    //     } else {
    //         static CHAIN: Lazy<AcceptSyscallFacade<NioAcceptSyscall<RawAcceptSyscall>>> =
    //             Lazy::new(Default::default);
    //     }
    // }
    static CHAIN: Lazy<AcceptSyscallFacade<NioAcceptSyscall<RawAcceptSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.accept(fn_ptr, fd, address, address_len)
}

trait AcceptSyscall {
    extern "system" fn accept(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, *mut SOCKADDR, *mut c_int) -> SOCKET>,
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
    ) -> SOCKET;
}

impl_facade!(AcceptSyscallFacade, AcceptSyscall,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);

impl_iocp!(IocpAcceptSyscall, AcceptSyscall,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);

impl_nio_read!(NioAcceptSyscall, AcceptSyscall,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);

impl_raw!(RawAcceptSyscall, AcceptSyscall, windows_sys::Win32::Networking::WinSock,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);
