use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{LPWSAOVERLAPPED_COMPLETION_ROUTINE, SOCKET, WSABUF};
use windows_sys::Win32::System::IO::OVERLAPPED;

trait WSASendSyscall {
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
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int;
}

impl_syscall!(WSASendSyscallFacade, NioWSASendSyscall, RawWSASendSyscall,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_facade!(WSASendSyscallFacade, WSASendSyscall,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_nio_write_iovec!(NioWSASendSyscall, WSASendSyscall,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_raw!(RawWSASendSyscall, WSASendSyscall, windows_sys::Win32::Networking::WinSock,
    WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);
