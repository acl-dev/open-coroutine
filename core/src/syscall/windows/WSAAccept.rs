use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::{LPCONDITIONPROC, SOCKADDR, SOCKET};

trait WSAAcceptSyscall {
    extern "system" fn WSAAccept(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                SOCKET,
                *mut SOCKADDR,
                *mut c_int,
                LPCONDITIONPROC,
                usize
            ) -> SOCKET
        >,
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        dwcallbackdata: usize,
    ) -> SOCKET;
}

impl_syscall!(WSAAcceptSyscallFacade, NioWSAAcceptSyscall, RawWSAAcceptSyscall,
    WSAAccept(
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        dwcallbackdata: usize
    ) -> SOCKET
);

impl_facade!(WSAAcceptSyscallFacade, WSAAcceptSyscall,
    WSAAccept(
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        dwcallbackdata: usize
    ) -> SOCKET
);

impl_nio_read!(NioWSAAcceptSyscall, WSAAcceptSyscall,
    WSAAccept(
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        dwcallbackdata: usize
    ) -> SOCKET
);

impl_raw!(RawWSAAcceptSyscall, WSAAcceptSyscall, windows_sys::Win32::Networking::WinSock,
    WSAAccept(
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        dwcallbackdata: usize
    ) -> SOCKET
);
