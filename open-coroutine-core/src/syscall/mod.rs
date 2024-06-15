#[cfg(target_os = "linux")]
use libc::epoll_event;
#[cfg(unix)]
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timespec,
    timeval,
};
#[cfg(unix)]
use std::ffi::{c_int, c_uint, c_void};

#[cfg(unix)]
pub mod common;

#[cfg(unix)]
pub mod raw;

#[cfg(unix)]
pub mod nio;

#[allow(unused_variables)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
pub mod io_uring;

#[cfg(unix)]
pub mod state;

#[cfg(unix)]
mod facade;
#[cfg(unix)]
pub use facade::*;

#[cfg(unix)]
pub trait UnixSyscall {
    /// sleep

    extern "C" fn sleep(
        &self,
        fn_ptr: Option<&extern "C" fn(c_uint) -> c_uint>,
        secs: c_uint,
    ) -> c_uint;

    extern "C" fn usleep(
        &self,
        fn_ptr: Option<&extern "C" fn(c_uint) -> c_int>,
        microseconds: c_uint,
    ) -> c_int;

    extern "C" fn nanosleep(
        &self,
        fn_ptr: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
        rqtp: *const timespec,
        rmtp: *mut timespec,
    ) -> c_int;

    /// poll

    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int;

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
    ) -> c_int;

    /// socket

    extern "C" fn socket(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> c_int;

    extern "C" fn listen(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        backlog: c_int,
    ) -> c_int;

    extern "C" fn accept(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
        socket: c_int,
        address: *mut sockaddr,
        address_len: *mut socklen_t,
    ) -> c_int;

    extern "C" fn connect(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
        socket: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> c_int;

    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        how: c_int,
    ) -> c_int;

    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int;

    /// read

    extern "C" fn recv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t;

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
    ) -> ssize_t;

    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
    ) -> ssize_t;

    extern "C" fn pread(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t;

    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;

    extern "C" fn preadv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t;

    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t;

    /// write

    extern "C" fn send(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t;

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
    ) -> ssize_t;

    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> ssize_t;

    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t;

    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t;

    extern "C" fn pwritev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t;

    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t;
}

#[cfg(target_os = "linux")]
pub trait LinuxSyscall: UnixSyscall {
    /// poll

    extern "C" fn epoll_ctl(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> c_int;

    /// socket

    extern "C" fn accept4(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int>,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> c_int;
}

#[allow(non_snake_case)]
#[cfg(windows)]
mod Sleep;
#[cfg(windows)]
pub use Sleep::Sleep;
