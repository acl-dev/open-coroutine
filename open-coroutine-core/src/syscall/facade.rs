use crate::syscall::nio::NioLinuxSyscall;
use crate::syscall::raw::RawLinuxSyscall;
use crate::syscall::state::StateLinuxSyscall;
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
        use crate::syscall::io_uring::IoUringLinuxSyscall;
        static CHAIN: Lazy<StateLinuxSyscall<IoUringLinuxSyscall<NioLinuxSyscall<RawLinuxSyscall>>>> =
            Lazy::new(StateLinuxSyscall::default);
    } else {
        static CHAIN: Lazy<StateLinuxSyscall<NioLinuxSyscall<RawLinuxSyscall>>> =
            Lazy::new(StateLinuxSyscall::default);
    }
}

/// write

#[must_use]
pub extern "C" fn sendto(
    fn_ptr: Option<
        &extern "C" fn(c_int, *const c_void, size_t, c_int, *const sockaddr, socklen_t) -> ssize_t,
    >,
    socket: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> ssize_t {
    CHAIN.sendto(fn_ptr, socket, buf, len, flags, addr, addrlen)
}

#[must_use]
pub extern "C" fn write(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    count: size_t,
) -> ssize_t {
    CHAIN.write(fn_ptr, fd, buf, count)
}

#[must_use]
pub extern "C" fn pwrite(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    CHAIN.pwrite(fn_ptr, fd, buf, count, offset)
}

#[must_use]
pub extern "C" fn sendmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *const msghdr,
    flags: c_int,
) -> ssize_t {
    CHAIN.sendmsg(fn_ptr, fd, msg, flags)
}

/// poll

#[cfg(target_os = "linux")]
#[must_use]
pub extern "C" fn epoll_ctl(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
    epfd: c_int,
    op: c_int,
    fd: c_int,
    event: *mut epoll_event,
) -> c_int {
    CHAIN.epoll_ctl(fn_ptr, epfd, op, fd, event)
}
