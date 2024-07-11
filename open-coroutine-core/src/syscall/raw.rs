#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use std::ffi::{c_int, c_void};

#[derive(Debug, Copy, Clone, Default)]
pub struct RawLinuxSyscall {}

impl UnixSyscall for RawLinuxSyscall {
    /// write

    extern "C" fn sendto(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *const c_void,
                size_t,
                c_int,
                *const sockaddr,
                socklen_t,
            ) -> ssize_t,
        >,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(socket, buf, len, flags, addr, addrlen)
        } else {
            unsafe { libc::sendto(socket, buf, len, flags, addr, addrlen) }
        }
    }

    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, buf, count)
        } else {
            unsafe { libc::write(fd, buf, count) }
        }
    }

    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, buf, count, offset)
        } else {
            unsafe { libc::pwrite(fd, buf, count, offset) }
        }
    }

    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, msg, flags)
        } else {
            unsafe { libc::sendmsg(fd, msg, flags) }
        }
    }
}

#[cfg(target_os = "linux")]
impl LinuxSyscall for RawLinuxSyscall {
    /// poll

    extern "C" fn epoll_ctl(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> c_int {
        if let Some(f) = fn_ptr {
            (f)(epfd, op, fd, event)
        } else {
            unsafe { libc::epoll_ctl(epfd, op, fd, event) }
        }
    }
}
