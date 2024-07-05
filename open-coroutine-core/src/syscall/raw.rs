#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timeval,
};
use std::ffi::{c_int, c_void};

#[derive(Debug, Copy, Clone, Default)]
pub struct RawLinuxSyscall {}

impl UnixSyscall for RawLinuxSyscall {
    /// poll

    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int {
        if let Some(f) = fn_ptr {
            (f)(fds, nfds, timeout)
        } else {
            unsafe { libc::poll(fds, nfds, timeout) }
        }
    }

    extern "C" fn select(
        &self,
        fn_ptr: Option<
            &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
        >,
        nfds: c_int,
        readfds: *mut fd_set,
        writefds: *mut fd_set,
        errorfds: *mut fd_set,
        timeout: *mut timeval,
    ) -> c_int {
        if let Some(f) = fn_ptr {
            (f)(nfds, readfds, writefds, errorfds, timeout)
        } else {
            unsafe { libc::select(nfds, readfds, writefds, errorfds, timeout) }
        }
    }

    /// socket

    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        how: c_int,
    ) -> c_int {
        if let Some(f) = fn_ptr {
            (f)(socket, how)
        } else {
            unsafe { libc::shutdown(socket, how) }
        }
    }

    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
        if let Some(f) = fn_ptr {
            (f)(fd)
        } else {
            unsafe { libc::close(fd) }
        }
    }

    /// read

    extern "C" fn recv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(socket, buf, len, flags)
        } else {
            unsafe { libc::send(socket, buf, len, flags) }
        }
    }

    extern "C" fn recvfrom(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *mut c_void,
                size_t,
                c_int,
                *mut sockaddr,
                *mut socklen_t,
            ) -> ssize_t,
        >,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(socket, buf, len, flags, addr, addrlen)
        } else {
            unsafe { libc::recvfrom(socket, buf, len, flags, addr, addrlen) }
        }
    }

    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, buf, count)
        } else {
            unsafe { libc::read(fd, buf, count) }
        }
    }

    extern "C" fn pread(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, buf, count, offset)
        } else {
            unsafe { libc::pread(fd, buf, count, offset) }
        }
    }

    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, iov, iovcnt)
        } else {
            unsafe { libc::readv(fd, iov, iovcnt) }
        }
    }

    extern "C" fn preadv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, iov, iovcnt, offset)
        } else {
            unsafe { libc::preadv(fd, iov, iovcnt, offset) }
        }
    }

    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, msg, flags)
        } else {
            unsafe { libc::recvmsg(fd, msg, flags) }
        }
    }

    /// write

    extern "C" fn send(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(socket, buf, len, flags)
        } else {
            unsafe { libc::send(socket, buf, len, flags) }
        }
    }

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

    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, iov, iovcnt)
        } else {
            unsafe { libc::writev(fd, iov, iovcnt) }
        }
    }

    extern "C" fn pwritev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        if let Some(f) = fn_ptr {
            (f)(fd, iov, iovcnt, offset)
        } else {
            unsafe { libc::pwritev(fd, iov, iovcnt, offset) }
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
