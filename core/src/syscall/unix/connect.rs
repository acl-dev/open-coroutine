use crate::common::now;
use crate::net::EventLoops;
use crate::syscall::{is_blocking, reset_errno, send_time_limit, set_blocking, set_errno, set_non_blocking};
use libc::{sockaddr, socklen_t};
use std::ffi::{c_int, c_void};
use std::io::Error;

trait ConnectSyscall {
    extern "C" fn connect(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
        fd: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> c_int;
}

impl_syscall!(ConnectSyscallFacade, IoUringConnectSyscall, NioConnectSyscall, RawConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);

impl_facade!(ConnectSyscallFacade, ConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);

impl_io_uring_write!(IoUringConnectSyscall, ConnectSyscall,
    connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int
);

#[repr(C)]
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
        let start_time = now();
        let mut left_time = send_time_limit(fd);
        let mut r = self.inner.connect(fn_ptr, fd, address, len);
        while left_time > 0 {
            if r == 0 {
                reset_errno();
                break;
            }
            let errno = Error::last_os_error().raw_os_error();
            if errno == Some(libc::EINPROGRESS) || errno == Some(libc::EALREADY) || errno == Some(libc::EWOULDBLOCK) {
                //阻塞，直到写事件发生
                left_time = start_time
                    .saturating_add(send_time_limit(fd))
                    .saturating_sub(now());
                let wait_time = std::time::Duration::from_nanos(left_time)
                    .min(crate::common::constants::SLICE);
                if EventLoops::wait_write_event(fd, Some(wait_time)).is_err()
                {
                    break;
                }
                let mut err = 0;
                unsafe {
                    let mut len = socklen_t::try_from(size_of_val(&err)).expect("overflow");
                    r = libc::getsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        std::ptr::addr_of_mut!(err).cast::<c_void>(),
                        &raw mut len,
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
                }
                unsafe {
                    let mut address = std::mem::zeroed();
                    let mut address_len = socklen_t::try_from(size_of_val(&address)).expect("overflow");
                    r = libc::getpeername(fd, &raw mut address, &raw mut address_len);
                }
            } else if errno != Some(libc::EINTR) {
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
