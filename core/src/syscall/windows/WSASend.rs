use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::{LPWSAOVERLAPPED_COMPLETION_ROUTINE, SOCKET, WSABUF};
use windows_sys::Win32::System::IO::OVERLAPPED;
use crate::common::constants::{CoroutineState, Syscall, SyscallState};
use crate::{error, info};
use crate::scheduler::SchedulableCoroutine;

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
    cfg_if::cfg_if! {
        if #[cfg(all(windows, feature = "iocp"))] {
            static CHAIN: Lazy<
                WSASendSyscallFacade<IocpWSASendSyscall<NioWSASendSyscall<RawWSASendSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<WSASendSyscallFacade<NioWSASendSyscall<RawWSASendSyscall>>> =
                Lazy::new(Default::default);
        }
    }
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
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int;
}

#[repr(C)]
#[derive(Debug, Default)]
struct WSASendSyscallFacade<I: WSASendSyscall> {
    inner: I,
}

impl<I: WSASendSyscall> WSASendSyscall for WSASendSyscallFacade<I> {
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
    ) -> c_int {
        let syscall = Syscall::WSASend;
        info!("enter syscall {}", syscall);
        if let Some(co) = SchedulableCoroutine::current() {
            _ = co.syscall((), syscall, SyscallState::Executing);
        }
        let r = self.inner.WSASend(
            fn_ptr,
            fd,
            buf,
            dwbuffercount,
            lpnumberofbytesrecvd,
            dwflags,
            lpoverlapped,
            lpcompletionroutine,
        );
        if let Some(co) = SchedulableCoroutine::current() {
            if let CoroutineState::SystemCall((), Syscall::WSASend, SyscallState::Executing) =
                co.state()
            {
                if co.running().is_err() {
                    error!("{} change to running state failed !", co.name());
                }
            }
        }
        info!("exit syscall {}", syscall);
        r
    }
}

#[cfg(all(windows, feature = "iocp"))]
#[repr(C)]
#[derive(Debug, Default)]
struct IocpWSASendSyscall<I: WSASendSyscall> {
    inner: I,
}

#[cfg(all(windows, feature = "iocp"))]
impl<I: WSASendSyscall> WSASendSyscall for IocpWSASendSyscall<I> {
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
    ) -> c_int {
        use windows_sys::Win32::Networking::WinSock::{SOCKET_ERROR, WSAEWOULDBLOCK};
        use crate::net::EventLoops;
        use crate::scheduler::SchedulableSuspender;

        if !lpoverlapped.is_null() {
            return RawWSASendSyscall::default().WSASend(
                fn_ptr,
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytesrecvd,
                dwflags,
                lpoverlapped,
                lpcompletionroutine,
            );
        }
        match EventLoops::WSASend(fd, buf, dwbuffercount, lpnumberofbytesrecvd, dwflags, lpoverlapped, lpcompletionroutine) {
            Ok(arc) => {
                if let Some(co) = SchedulableCoroutine::current() {
                    if let CoroutineState::SystemCall((), syscall, SyscallState::Executing) = co.state()
                    {
                        let new_state = SyscallState::Suspend(u64::MAX);
                        if co.syscall((), syscall, new_state).is_err() {
                            error!(
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
                            error!(
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
                    self.inner.WSASend(
                        fn_ptr,
                        fd,
                        buf,
                        dwbuffercount,
                        lpnumberofbytesrecvd,
                        dwflags,
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

impl_nio_write_iovec!(NioWSASendSyscall, WSASendSyscall,
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

impl_raw!(RawWSASendSyscall, WSASendSyscall, windows_sys::Win32::Networking::WinSock,
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
