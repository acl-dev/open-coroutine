use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::{IPPROTO, SOCKET, WINSOCK_SOCKET_TYPE};

trait SocketSyscall {
    extern "system" fn socket(
        &self,
        fn_ptr: Option<&extern "system" fn(c_int, WINSOCK_SOCKET_TYPE, IPPROTO) -> SOCKET>,
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
    ) -> SOCKET;
}

impl_syscall!(SocketSyscallFacade, NioSocketSyscall, RawSocketSyscall,
    socket(domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET
);

impl_facade!(SocketSyscallFacade, SocketSyscall,
    socket(domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioSocketSyscall<I: SocketSyscall> {
    inner: I,
}

impl<I: SocketSyscall> SocketSyscall for NioSocketSyscall<I> {
    extern "system" fn socket(
        &self,
        fn_ptr: Option<&extern "system" fn(c_int, WINSOCK_SOCKET_TYPE, IPPROTO) -> SOCKET>,
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
    ) -> SOCKET {
        cfg_if::cfg_if! {
            if #[cfg(all(windows, feature = "iocp"))] {
                // `socket` cannot express `WSA_FLAG_OVERLAPPED`; go through
                // `WSASocketW` so the socket supports overlapped I/O.
                let _ = fn_ptr;
                let r = unsafe {
                    windows_sys::Win32::Networking::WinSock::WSASocketW(
                        domain,
                        ty,
                        protocol,
                        std::ptr::null(),
                        0,
                        windows_sys::Win32::Networking::WinSock::WSA_FLAG_OVERLAPPED,
                    )
                };
                if windows_sys::Win32::Networking::WinSock::INVALID_SOCKET != r {
                    _ = crate::net::operator::OVERLAPPED_SOCKETS.insert(r);
                }
                r
            } else {
                self.inner.socket(fn_ptr, domain, ty, protocol)
            }
        }
    }
}

impl_raw!(RawSocketSyscall, SocketSyscall, windows_sys::Win32::Networking::WinSock,
    socket(domain: c_int, ty: WINSOCK_SOCKET_TYPE, protocol: IPPROTO) -> SOCKET
);
