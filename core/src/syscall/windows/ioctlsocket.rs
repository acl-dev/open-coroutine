use crate::common::constants::{CoroutineState, Syscall, SyscallState};
use crate::scheduler::SchedulableCoroutine;
use crate::syscall::windows::NON_BLOCKING;
use crate::{error, info};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint};
use windows_sys::Win32::Networking::WinSock::SOCKET;

#[must_use]
pub extern "system" fn ioctlsocket(
    fn_ptr: Option<&extern "system" fn(SOCKET, c_int, *mut c_uint) -> c_int>,
    fd: SOCKET,
    cmd: c_int,
    argp: *mut c_uint,
) -> c_int {
    static CHAIN: Lazy<IoctlsocketSyscallFacade<NioIoctlsocketSyscall<RawIoctlsocketSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.ioctlsocket(fn_ptr, fd, cmd, argp)
}

trait IoctlsocketSyscall {
    extern "system" fn ioctlsocket(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int, *mut c_uint) -> c_int>,
        fd: SOCKET,
        cmd: c_int,
        argp: *mut c_uint,
    ) -> c_int;
}

#[repr(C)]
#[derive(Debug, Default)]
struct IoctlsocketSyscallFacade<I: IoctlsocketSyscall> {
    inner: I,
}

impl<I: IoctlsocketSyscall> IoctlsocketSyscall for IoctlsocketSyscallFacade<I> {
    extern "system" fn ioctlsocket(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int, *mut c_uint) -> c_int>,
        fd: SOCKET,
        cmd: c_int,
        argp: *mut c_uint,
    ) -> c_int {
        let syscall = Syscall::ioctlsocket;
        info!("enter syscall {}", syscall);
        if let Some(co) = SchedulableCoroutine::current() {
            _ = co.syscall((), syscall, SyscallState::Executing);
        }
        let r = self.inner.ioctlsocket(fn_ptr, fd, cmd, argp);
        if let Some(co) = SchedulableCoroutine::current() {
            if let CoroutineState::SystemCall((), Syscall::ioctlsocket, SyscallState::Executing) =
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

#[repr(C)]
#[derive(Debug, Default)]
struct NioIoctlsocketSyscall<I: IoctlsocketSyscall> {
    inner: I,
}

impl<I: IoctlsocketSyscall> IoctlsocketSyscall for NioIoctlsocketSyscall<I> {
    extern "system" fn ioctlsocket(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, c_int, *mut c_uint) -> c_int>,
        fd: SOCKET,
        cmd: c_int,
        argp: *mut c_uint,
    ) -> c_int {
        let r = self.inner.ioctlsocket(fn_ptr, fd, cmd, argp);
        if 0 == r {
            if 0 == unsafe { *argp } {
                _ = NON_BLOCKING.remove(&fd);
            } else {
                _ = NON_BLOCKING.insert(fd);
            }
        }
        r
    }
}

impl_raw!(RawIoctlsocketSyscall, IoctlsocketSyscall, windows_sys::Win32::Networking::WinSock,
    ioctlsocket(fd: SOCKET, cmd: c_int, argp: *mut c_uint) -> c_int
);
