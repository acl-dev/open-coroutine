use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{IPPROTO, SOCKET, WINSOCK_SOCKET_TYPE, WSAPROTOCOL_INFOW};

#[must_use]
pub extern "system" fn WSASocketW(
    fn_ptr: Option<
        &extern "system" fn(
            c_int,
            WINSOCK_SOCKET_TYPE,
            IPPROTO,
            *const WSAPROTOCOL_INFOW,
            c_uint,
            c_uint,
        ) -> SOCKET,
    >,
    domain: c_int,
    ty: WINSOCK_SOCKET_TYPE,
    protocol: IPPROTO,
    lpprotocolinfo: *const WSAPROTOCOL_INFOW,
    g: c_uint,
    dw_flags: c_uint,
) -> SOCKET {
    static CHAIN: Lazy<WSASocketWSyscallFacade<NioWSASocketWSyscall<RawWSASocketWSyscall>>> = Lazy::new(Default::default);
    CHAIN.WSASocketW(fn_ptr, domain, ty, protocol, lpprotocolinfo, g, dw_flags)
}

trait WSASocketWSyscall {
    extern "system" fn WSASocketW(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                c_int,
                WINSOCK_SOCKET_TYPE,
                IPPROTO,
                *const WSAPROTOCOL_INFOW,
                c_uint,
                c_uint,
            ) -> SOCKET,
        >,
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint,
    ) -> SOCKET;
}

impl_facade!(WSASocketWSyscallFacade, WSASocketWSyscall,
    WSASocketW(
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint
    ) -> SOCKET
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioWSASocketWSyscall<I: WSASocketWSyscall> {
    inner: I,
}

impl<I: WSASocketWSyscall> WSASocketWSyscall for NioWSASocketWSyscall<I> {
    extern "system" fn WSASocketW(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                c_int,
                WINSOCK_SOCKET_TYPE,
                IPPROTO,
                *const WSAPROTOCOL_INFOW,
                c_uint,
                c_uint,
            ) -> SOCKET,
        >,
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint
    ) -> SOCKET {
        let r = self.inner.WSASocketW(fn_ptr, domain, ty, protocol, lpprotocolinfo, g, dw_flags);
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

impl_raw!(RawWSASocketWSyscall, WSASocketWSyscall, windows_sys::Win32::Networking::WinSock,
    WSASocketW(
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint
    ) -> SOCKET
);
