use crate::net::event_loop::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_errno, set_non_blocking};
use libc::{sockaddr, socklen_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};
use std::io::Error;
use std::time::Duration;

#[must_use]
pub extern "C" fn connect(
    fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
    socket: c_int,
    address: *const sockaddr,
    len: socklen_t,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                ConnectSyscallFacade<IoUringConnectSyscall<NioConnectSyscall<RawConnectSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<ConnectSyscallFacade<NioConnectSyscall<RawConnectSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.connect(fn_ptr, socket, address, len)
}

trait ConnectSyscall {
    extern "C" fn connect(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
        fd: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> c_int;
}

impl_facade!(ConnectSyscallFacade, ConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);

impl_io_uring!(IoUringConnectSyscall, ConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);

#[derive(Debug, Default)]
struct NioConnectSyscall<I: ConnectSyscall> {
    inner: I,
}

impl<I: ConnectSyscall> ConnectSyscall for NioConnectSyscall<I> {
    extern "C" fn connect(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
        fd: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> c_int {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let mut r = self.inner.connect(fn_ptr, fd, address, len);
        if r == 0 {
            reset_errno();
            return r;
        }
        loop {
            let errno = Error::last_os_error().raw_os_error();
            if errno == Some(libc::EINPROGRESS) || errno == Some(libc::ENOTCONN) {
                //阻塞，直到写事件发生
                if EventLoops::wait_write_event(fd, Some(Duration::from_millis(10))).is_err() {
                    r = -1;
                    break;
                }
                let mut err: c_int = 0;
                unsafe {
                    let mut len: socklen_t = std::mem::zeroed();
                    r = libc::getsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        (std::ptr::addr_of_mut!(err)).cast::<c_void>(),
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
                    r = libc::getpeername(fd, &mut address, &mut address_len);
                }
                if r == 0 {
                    reset_errno();
                    r = 0;
                    break;
                }
            } else if errno != Some(libc::EINTR) {
                r = -1;
                break;
            }
        }
        if r == -1 && Error::last_os_error().raw_os_error() == Some(libc::ETIMEDOUT) {
            set_errno(libc::EINPROGRESS);
        }
        if blocking {
            set_blocking(fd);
        }
        r
    }
}

impl_raw!(RawConnectSyscall, ConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);
