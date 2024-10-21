use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{LPWSAOVERLAPPED_COMPLETION_ROUTINE, SOCKET, WSABUF};
use windows_sys::Win32::System::IO::OVERLAPPED;

#[must_use]
pub extern "system" fn WSASend(
    fn_ptr: Option<
        &extern "system" fn(
            SOCKET,
            *const WSABUF,
            c_uint,
            *mut c_uint,
            c_uint,
            *mut OVERLAPPED,
            LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        ) -> c_int,
    >,
    fd: SOCKET,
    buf: *const WSABUF,
    dwbuffercount: c_uint,
    lpnumberofbytesrecvd: *mut c_uint,
    dwflags: c_uint,
    lpoverlapped: *mut OVERLAPPED,
    lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> c_int {
    static CHAIN: Lazy<WSARecvSyscallFacade<NioWSARecvSyscall<RawWSARecvSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.WSASend(
        fn_ptr,
        fd,
        buf,
        dwbuffercount,
        lpnumberofbytesrecvd,
        dwflags,
        lpoverlapped,
        lpcompletionroutine,
    )
}

trait WSARecvSyscall {
    extern "system" fn WSASend(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                SOCKET,
                *const WSABUF,
                c_uint,
                *mut c_uint,
                c_uint,
                *mut OVERLAPPED,
                LPWSAOVERLAPPED_COMPLETION_ROUTINE,
            ) -> c_int,
        >,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int;
}

impl_facade!(WSARecvSyscallFacade, WSARecvSyscall,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags : c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_nio_write_iovec!(NioWSARecvSyscall, WSARecvSyscall,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags : c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_raw!(RawWSARecvSyscall, WSARecvSyscall, windows_sys::Win32::Networking::WinSock,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags : c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);
