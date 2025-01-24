use std::convert::TryInto;
use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};

#[must_use]
pub extern "system" fn accept(
    fn_ptr: Option<&extern "system" fn(SOCKET, *mut SOCKADDR, *mut c_int) -> SOCKET>,
    fd: SOCKET,
    address: *mut SOCKADDR,
    address_len: *mut c_int,
) -> SOCKET {
    cfg_if::cfg_if! {
        if #[cfg(feature = "iocp")] {
            static CHAIN: Lazy<
                AcceptSyscallFacade<IocpAcceptSyscall<NioAcceptSyscall<RawAcceptSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<AcceptSyscallFacade<NioAcceptSyscall<RawAcceptSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.accept(fn_ptr, fd, address, address_len)
}

trait AcceptSyscall {
    extern "system" fn accept(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, *mut SOCKADDR, *mut c_int) -> SOCKET>,
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int,
    ) -> SOCKET;
}

impl_facade!(AcceptSyscallFacade, AcceptSyscall,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);

#[repr(C)]
#[derive(Debug, Default)]
#[cfg(all(windows, feature = "iocp"))]
struct IocpAcceptSyscall<I: AcceptSyscall> {
    inner: I,
}

#[cfg(all(windows, feature = "iocp"))]
impl<I: AcceptSyscall> AcceptSyscall for IocpAcceptSyscall<I> {
    extern "system" fn accept(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, *mut SOCKADDR, *mut c_int) -> SOCKET>,
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int
    ) -> SOCKET {
        use crate::common::constants::{CoroutineState, SyscallState};
        use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};
        use windows_sys::Win32::Networking::WinSock::{INVALID_SOCKET, getsockopt, SOL_SOCKET, SO_PROTOCOL_INFO, WSAPROTOCOL_INFOW};

        if let Ok(arc) = crate::net::EventLoops::accept(fd, address, address_len) {
            if let Some(co) = SchedulableCoroutine::current() {
                if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                {
                    let new_state = SyscallState::Suspend(crate::syscall::recv_time_limit(fd));
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
            if let CoroutineState::Syscall((), syscall, SyscallState::Callback) = co.state()
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
            let syscall_result = cvar
                .wait_while(lock.lock().expect("lock failed"),
                    |&mut result| result.is_none()
                )
                .expect("lock failed")
                .expect("no syscall result");
            if syscall_result < 0 {
                crate::syscall::set_errno((-syscall_result).try_into().expect("errno overflow"));
                return INVALID_SOCKET;
            }
            unsafe {
                let mut sock_info: WSAPROTOCOL_INFOW = std::mem::zeroed();
                let mut sock_info_len = size_of::<WSAPROTOCOL_INFOW>()
                    .try_into()
                    .expect("sock_info_len overflow");
                if getsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_PROTOCOL_INFO,
                    std::ptr::from_mut(&mut sock_info).cast(),
                    &mut sock_info_len,
                ) != 0
                {
                    return INVALID_SOCKET;
                }
                (*address).sa_family = sock_info.iAddressFamily.try_into().expect("iAddressFamily overflow");
            }
            return SOCKET::try_from(syscall_result).expect("overflow");
        }
        self.inner.accept(fn_ptr, fd, address, address_len)
    }
}

impl_nio_read!(NioAcceptSyscall, AcceptSyscall,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);

impl_raw!(RawAcceptSyscall, AcceptSyscall, windows_sys::Win32::Networking::WinSock,
    accept(fd: SOCKET, address: *mut SOCKADDR, address_len: *mut c_int) -> SOCKET
);
