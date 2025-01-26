use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{
    IPPROTO, SOCKET, WINSOCK_SOCKET_TYPE, WSAPROTOCOL_INFOW,
};

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

impl_syscall!(WSASocketWSyscallFacade, RawWSASocketWSyscall,
    WSASocketW(
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint
    ) -> SOCKET
);

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
