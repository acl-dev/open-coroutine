use crate::net::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_errno, set_non_blocking};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use std::io::Error;
use windows_sys::Win32::Networking::WinSock::{getpeername, getsockopt, SO_ERROR, SOCKADDR, SOCKET, SOL_SOCKET, WSAEALREADY, WSAEINPROGRESS, WSAEINTR, WSAETIMEDOUT};

#[must_use]
pub extern "system" fn connect(
    fn_ptr: Option<&extern "system" fn(SOCKET, *const SOCKADDR, c_int) -> c_int>,
    socket: SOCKET,
    address: *const SOCKADDR,
    len: c_int,
) -> c_int {
    static CHAIN: Lazy<ConnectSyscallFacade<NioConnectSyscall<RawConnectSyscall>>> =
        Lazy::new(Default::default);
    CHAIN.connect(fn_ptr, socket, address, len)
}

trait ConnectSyscall {
    extern "system" fn connect(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, *const SOCKADDR, c_int) -> c_int>,
        fd: SOCKET,
        address: *const SOCKADDR,
        len: c_int,
    ) -> c_int;
}

impl_facade!(ConnectSyscallFacade, ConnectSyscall,
    connect(fd: SOCKET, address: *const SOCKADDR, len: c_int) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioConnectSyscall<I: ConnectSyscall> {
    inner: I,
}

impl<I: ConnectSyscall> ConnectSyscall for NioConnectSyscall<I> {
    extern "system" fn connect(
        &self,
        fn_ptr: Option<&extern "system" fn(SOCKET, *const SOCKADDR, c_int) -> c_int>,
        fd: SOCKET,
        address: *const SOCKADDR,
        len: c_int,
    ) -> c_int {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let mut r = self.inner.connect(fn_ptr, fd, address, len);
        loop {
            if r == 0 {
                reset_errno();
                break;
            }
            let errno = Error::last_os_error().raw_os_error();
            if errno == Some(WSAEINPROGRESS) || errno == Some(WSAEALREADY) {
                //阻塞，直到写事件发生
                if EventLoops::wait_write_event(
                    fd as _,
                    Some(crate::common::constants::SLICE)
                ).is_err() {
                    break;
                }
                let mut err = 0;
                unsafe {
                    let mut len: c_int = std::mem::zeroed();
                    r = getsockopt(
                        fd,
                        SOL_SOCKET,
                        SO_ERROR,
                        std::ptr::addr_of_mut!(err).cast::<u8>(),
                        &mut len,
                    );
                }
                if r != 0 {
                    r = -1;
                    break;
                }
                if err != 0 {
                    set_errno(err);
                    r = -1;
                    break;
                };
                unsafe {
                    let mut address = std::mem::zeroed();
                    let mut address_len = std::mem::zeroed();
                    r = getpeername(fd, &mut address, &mut address_len);
                }
            } else if errno != Some(WSAEINTR) {
                break;
            }
        }
        if r == -1 && Error::last_os_error().raw_os_error() == Some(WSAETIMEDOUT) {
            set_errno(WSAEINPROGRESS.try_into().expect("overflow"));
        }
        if blocking {
            set_blocking(fd);
        }
        r
    }
}

impl_raw!(RawConnectSyscall, ConnectSyscall, windows_sys::Win32::Networking::WinSock,
    connect(fd: SOCKET, address: *const SOCKADDR, len: c_int) -> c_int
);
