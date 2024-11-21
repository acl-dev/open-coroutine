use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::{IPPROTO, SOCKET, WINSOCK_SOCKET_TYPE};

#[must_use]
pub extern "system" fn socket(
    fn_ptr: Option<&extern "system" fn(c_int, WINSOCK_SOCKET_TYPE, IPPROTO) -> SOCKET>,
    domain: c_int,
    ty: WINSOCK_SOCKET_TYPE,
    protocol: IPPROTO,
) -> SOCKET {
    static CHAIN: Lazy<SocketSyscallFacade<NioSocketSyscall<RawSocketSyscall>>> = Lazy::new(Default::default);
    CHAIN.socket(fn_ptr, domain, ty, protocol)
}

trait SocketSyscall {
    extern "system" fn socket(
        &self,
        fn_ptr: Option<&extern "system" fn(c_int, WINSOCK_SOCKET_TYPE, IPPROTO) -> SOCKET>,
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
    ) -> SOCKET;
}

impl_facade!(SocketSyscallFacade, SocketSyscall,
    socket(domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioSocketSyscall<I: SocketSyscall> {
    inner: I,
}

impl<I: SocketSyscall> SocketSyscall for NioSocketSyscall<I> {
    extern "system" fn socket(&self, fn_ptr: Option<&extern "system" fn(c_int, WINSOCK_SOCKET_TYPE, IPPROTO) -> SOCKET>, domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET {
        let r = self.inner.socket(fn_ptr, domain, ty, protocol);
        #[cfg(feature = "iocp")]
        if windows_sys::Win32::Networking::WinSock::INVALID_SOCKET != r {
            _ = crate::net::operator::SOCKET_CONTEXT.insert(r,crate::net::operator::SocketContext{
                domain,
                ty,
                protocol,
            });
        }
        r
    }
}

impl_raw!(RawSocketSyscall, SocketSyscall, windows_sys::Win32::Networking::WinSock,
    socket(domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET
);
