use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{LPWSAOVERLAPPED_COMPLETION_ROUTINE, SOCKET, WSABUF};
use windows_sys::Win32::System::IO::OVERLAPPED;

#[must_use]
pub extern "system" fn WSARecv(
    fn_ptr: Option<
        &extern "system" fn(
            SOCKET,
            *const WSABUF,
            c_uint,
            *mut c_uint,
            *mut c_uint,
            *mut OVERLAPPED,
            LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        ) -> c_int,
    >,
    fd: SOCKET,
    buf: *const WSABUF,
    dwbuffercount: c_uint,
    lpnumberofbytesrecvd: *mut c_uint,
    lpflags: *mut c_uint,
    lpoverlapped: *mut OVERLAPPED,
    lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(windows, feature = "iocp"))] {
            static CHAIN: Lazy<
                WSARecvSyscallFacade<IocpWSARecvSyscall<NioWSARecvSyscall<RawWSARecvSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<WSARecvSyscallFacade<NioWSARecvSyscall<RawWSARecvSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.WSARecv(
        fn_ptr,
        fd,
        buf,
        dwbuffercount,
        lpnumberofbytesrecvd,
        lpflags,
        lpoverlapped,
        lpcompletionroutine,
    )
}

trait WSARecvSyscall {
    extern "system" fn WSARecv(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                SOCKET,
                *const WSABUF,
                c_uint,
                *mut c_uint,
                *mut c_uint,
                *mut OVERLAPPED,
                LPWSAOVERLAPPED_COMPLETION_ROUTINE,
            ) -> c_int,
        >,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int;
}

impl_facade!(WSARecvSyscallFacade, WSARecvSyscall,
    WSARecv(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags : *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

#[cfg(all(windows, feature = "iocp"))]
#[repr(C)]
#[derive(Debug, Default)]
struct IocpWSARecvSyscall<I: WSARecvSyscall> {
    inner: I,
}

#[cfg(all(windows, feature = "iocp"))]
impl<I: WSARecvSyscall> WSARecvSyscall for IocpWSARecvSyscall<I> {
    #[allow(clippy::too_many_lines)]
    extern "system" fn WSARecv(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                SOCKET,
                *const WSABUF,
                c_uint,
                *mut c_uint,
                *mut c_uint,
                *mut OVERLAPPED,
                LPWSAOVERLAPPED_COMPLETION_ROUTINE,
            ) -> c_int,
        >,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int {
        use windows_sys::Win32::Networking::WinSock::{SOCKET_ERROR, WSAEWOULDBLOCK};
        use crate::common::constants::{CoroutineState, SyscallState};
        use crate::net::EventLoops;
        use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

        if !lpoverlapped.is_null() {
            return RawWSARecvSyscall::default().WSARecv(
                fn_ptr,
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytesrecvd,
                lpflags,
                lpoverlapped,
                lpcompletionroutine,
            );
        }
        match EventLoops::WSARecv(fd, buf, dwbuffercount, lpnumberofbytesrecvd, lpflags, lpoverlapped, lpcompletionroutine) {
            Ok(arc) => {
                if let Some(co) = SchedulableCoroutine::current() {
                    if let CoroutineState::SystemCall((), syscall, SyscallState::Executing) = co.state()
                    {
                        let new_state = SyscallState::Suspend(u64::MAX);
                        if co.syscall((), syscall, new_state).is_err() {
                            crate::error!(
                                "{} change to syscall {} {} failed !",
                                co.name(), syscall, new_state
                            );
                        }
                    }
                }
                if let Some(suspender) = SchedulableSuspender::current() {
                    suspender.suspend();
                    //回来的时候，系统调用已经执行完了
                }
                if let Some(co) = SchedulableCoroutine::current() {
                    if let CoroutineState::SystemCall((), syscall, SyscallState::Callback) = co.state()
                    {
                        let new_state = SyscallState::Executing;
                        if co.syscall((), syscall, new_state).is_err() {
                            crate::error!(
                                "{} change to syscall {} {} failed !",
                                co.name(), syscall, new_state
                            );
                        }
                    }
                }
                let (lock, cvar) = &*arc;
                let syscall_result: c_int = cvar
                    .wait_while(lock.lock().expect("lock failed"),
                                |&mut result| result.is_none()
                    )
                    .expect("lock failed")
                    .expect("no syscall result")
                    .try_into()
                    .expect("IOCP syscall result overflow");
                // fixme 错误处理
                // if syscall_result < 0 {
                //     let errno: std::ffi::c_int = (-syscall_result).try_into()
                //         .expect("IOCP errno overflow");
                //     $crate::syscall::common::set_errno(errno);
                //     syscall_result = -1;
                // }
                syscall_result
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Other {
                    self.inner.WSARecv(
                        fn_ptr,
                        fd,
                        buf,
                        dwbuffercount,
                        lpnumberofbytesrecvd,
                        lpflags,
                        lpoverlapped,
                        lpcompletionroutine,
                    )
                } else {
                    crate::syscall::common::set_errno(WSAEWOULDBLOCK.try_into().expect("overflow"));
                    SOCKET_ERROR
                }
            }
        }
    }
}

impl_nio_read_iovec!(NioWSARecvSyscall, WSARecvSyscall,
    WSARecv(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags : *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);

impl_raw!(RawWSARecvSyscall, WSARecvSyscall, windows_sys::Win32::Networking::WinSock,
    WSARecv(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags : *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int
);
