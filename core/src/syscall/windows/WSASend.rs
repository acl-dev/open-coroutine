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

impl_syscall!(WSASendSyscallFacade, IocpWSASendSyscall, NioWSASendSyscall, RawWSASendSyscall,
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

#[repr(C)]
#[derive(Debug, Default)]
#[cfg(all(windows, feature = "iocp"))]
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
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int {
        use crate::common::constants::{CoroutineState, SyscallState};
        use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

        if let Ok(arc) = crate::net::EventLoops::WSASend(
            fd,
            buf,
            dwbuffercount,
            lpnumberofbytessent,
            dwflags,
            lpoverlapped,
            lpcompletionroutine,
        ) {
            if let Some(co) = SchedulableCoroutine::current() {
                if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                {
                    let new_state = SyscallState::Suspend(
                        crate::common::now()
                            .saturating_add(crate::syscall::send_time_limit(fd)),
                    );
                    if co.syscall((), syscall, new_state).is_err() {
                        crate::error!(
                            "{} change to syscall {} {} failed !",
                            co.name(),
                            syscall,
                            new_state
                        );
                    }
                }
            }
            if let Some(suspender) = SchedulableSuspender::current() {
                suspender.suspend();
                //回来的时候，系统调用已经执行完了
            }
            if let Some(co) = SchedulableCoroutine::current() {
                if let CoroutineState::Syscall((), syscall, syscall_state) = co.state() {
                    match syscall_state {
                        SyscallState::Timeout => {
                            crate::syscall::set_errno(
                                windows_sys::Win32::Networking::WinSock::WSAETIMEDOUT
                                    .try_into()
                                    .expect("errno overflow"),
                            );
                            return -1;
                        }
                        SyscallState::Callback => {
                            let new_state = SyscallState::Executing;
                            if co.syscall((), syscall, new_state).is_err() {
                                crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(),
                                    syscall,
                                    new_state
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            let (lock, cvar) = &*arc;
            let syscall_result = cvar
                .wait_while(lock.lock().expect("lock failed"), |&mut result| {
                    result.is_none()
                })
                .expect("lock failed")
                .expect("no syscall result");
            if syscall_result < 0 {
                crate::syscall::set_errno(
                    (-syscall_result).try_into().expect("errno overflow"),
                );
                return -1;
            }
            // WSASend reports the transferred byte count through
            // `lpnumberofbytessent` and returns 0 on success.
            if !lpnumberofbytessent.is_null() {
                unsafe {
                    *lpnumberofbytessent =
                        c_uint::try_from(syscall_result).expect("bytes overflow");
                }
            }
            return 0;
        }
        self.inner.WSASend(
            fn_ptr,
            fd,
            buf,
            dwbuffercount,
            lpnumberofbytessent,
            dwflags,
            lpoverlapped,
            lpcompletionroutine,
        )
    }
}

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
