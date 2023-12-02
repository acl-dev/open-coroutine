use crate::common::Current;
use crate::coroutine::StateCoroutine;
use crate::impl_read_hook;
use crate::net::event_loop::EventLoops;
use crate::syscall::common::{reset_errno, set_errno};
use crate::syscall::raw::RawLinuxSyscall;
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timespec,
    timeval,
};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint, c_void};
use std::time::Duration;

static CHAIN: Lazy<RawLinuxSyscall> = Lazy::new(RawLinuxSyscall::default);

/// sleep

#[must_use]
pub extern "C" fn sleep(_: Option<&extern "C" fn(c_uint) -> c_uint>, secs: c_uint) -> c_uint {
    crate::info!("sleep hooked");
    _ = EventLoops::wait_event(Some(Duration::from_secs(u64::from(secs))));
    reset_errno();
    0
}

#[must_use]
pub extern "C" fn usleep(
    _: Option<&extern "C" fn(c_uint) -> c_int>,
    microseconds: c_uint,
) -> c_int {
    crate::info!("usleep hooked");
    let time = match u64::from(microseconds).checked_mul(1_000) {
        Some(v) => Duration::from_nanos(v),
        None => Duration::MAX,
    };
    _ = EventLoops::wait_event(Some(time));
    reset_errno();
    0
}

#[must_use]
pub extern "C" fn nanosleep(
    _: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
    rqtp: *const timespec,
    rmtp: *mut timespec,
) -> c_int {
    crate::info!("nanosleep hooked");
    let rqtp = unsafe { *rqtp };
    if rqtp.tv_sec < 0 || rqtp.tv_nsec < 0 || rqtp.tv_nsec > 999_999_999 {
        set_errno(libc::EINVAL);
        return -1;
    }
    //等待事件到来
    _ = EventLoops::wait_event(Some(Duration::new(rqtp.tv_sec as u64, rqtp.tv_nsec as u32)));
    reset_errno();
    if !rmtp.is_null() {
        unsafe {
            (*rmtp).tv_sec = 0;
            (*rmtp).tv_nsec = 0;
        }
    }
    0
}

/// poll

#[must_use]
pub extern "C" fn poll(
    fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
    fds: *mut pollfd,
    nfds: nfds_t,
    timeout: c_int,
) -> c_int {
    unbreakable!(
        {
            let mut t = if timeout < 0 { c_int::MAX } else { timeout };
            let mut x = 1;
            let mut r;
            // just check select every x ms
            loop {
                r = CHAIN.poll(fn_ptr, fds, nfds, 0);
                if r != 0 || t == 0 {
                    break;
                }
                _ = EventLoops::wait_event(Some(Duration::from_millis(t.min(x) as u64)));
                if t != c_int::MAX {
                    t = if t > x { t - x } else { 0 };
                }
                if x < 16 {
                    x <<= 1;
                }
            }
            r
        },
        poll
    )
}

#[must_use]
pub extern "C" fn select(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
    >,
    nfds: c_int,
    readfds: *mut fd_set,
    writefds: *mut fd_set,
    errorfds: *mut fd_set,
    timeout: *mut timeval,
) -> c_int {
    unbreakable!(
        {
            let mut t = if timeout.is_null() {
                c_uint::MAX
            } else {
                unsafe { ((*timeout).tv_sec as c_uint) * 1_000_000 + (*timeout).tv_usec as c_uint }
            };
            let mut o = timeval {
                tv_sec: 0,
                tv_usec: 0,
            };
            let mut s: [fd_set; 3] = unsafe { std::mem::zeroed() };
            unsafe {
                if !readfds.is_null() {
                    s[0] = *readfds;
                }
                if !writefds.is_null() {
                    s[1] = *writefds;
                }
                if !errorfds.is_null() {
                    s[2] = *errorfds;
                }
            }
            let mut x = 1;
            let mut r;
            // just check poll every x ms
            loop {
                r = CHAIN.select(fn_ptr, nfds, readfds, writefds, errorfds, &mut o);
                if r != 0 || t == 0 {
                    break;
                }
                _ = EventLoops::wait_event(Some(Duration::from_millis(u64::from(t.min(x)))));
                if t != c_uint::MAX {
                    t = if t > x { t - x } else { 0 };
                }
                if x < 16 {
                    x <<= 1;
                }
                unsafe {
                    if !readfds.is_null() {
                        *readfds = s[0];
                    }
                    if !writefds.is_null() {
                        *writefds = s[1];
                    }
                    if !errorfds.is_null() {
                        *errorfds = s[2];
                    }
                }
                o.tv_sec = 0;
                o.tv_usec = 0;
            }
            r
        },
        select
    )
}

/// socket

#[must_use]
pub extern "C" fn socket(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
    domain: c_int,
    ty: c_int,
    protocol: c_int,
) -> c_int {
    unbreakable!(CHAIN.socket(fn_ptr, domain, ty, protocol), socket)
}

#[must_use]
pub extern "C" fn listen(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    backlog: c_int,
) -> c_int {
    unbreakable!(CHAIN.listen(fn_ptr, socket, backlog), listen)
}

#[must_use]
pub extern "C" fn accept(
    fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
    socket: c_int,
    address: *mut sockaddr,
    address_len: *mut socklen_t,
) -> c_int {
    impl_read_hook!(CHAIN, accept, fn_ptr, socket, address, address_len)
}

#[must_use]
pub extern "C" fn connect(
    fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
    socket: c_int,
    address: *const sockaddr,
    len: socklen_t,
) -> c_int {
    //todo
    CHAIN.connect(fn_ptr, socket, address, len)
}

#[must_use]
pub extern "C" fn shutdown(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    how: c_int,
) -> c_int {
    CHAIN.shutdown(fn_ptr, socket, how)
}

#[must_use]
pub extern "C" fn close(fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
    CHAIN.close(fn_ptr, fd)
}

/// read

#[must_use]
pub extern "C" fn recv(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    CHAIN.recv(fn_ptr, socket, buf, len, flags)
}

#[must_use]
pub extern "C" fn recvfrom(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut c_void, size_t, c_int, *mut sockaddr, *mut socklen_t) -> ssize_t,
    >,
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> ssize_t {
    CHAIN.recvfrom(fn_ptr, socket, buf, len, flags, addr, addrlen)
}

#[must_use]
pub extern "C" fn read(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    count: size_t,
) -> ssize_t {
    CHAIN.read(fn_ptr, fd, buf, count)
}

#[must_use]
pub extern "C" fn pread(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    CHAIN.pread(fn_ptr, fd, buf, count, offset)
}

#[must_use]
pub extern "C" fn readv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    CHAIN.readv(fn_ptr, fd, iov, iovcnt)
}

#[must_use]
pub extern "C" fn preadv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    CHAIN.preadv(fn_ptr, fd, iov, iovcnt, offset)
}

#[must_use]
pub extern "C" fn recvmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *mut msghdr,
    flags: c_int,
) -> ssize_t {
    CHAIN.recvmsg(fn_ptr, fd, msg, flags)
}

/// write

#[must_use]
pub extern "C" fn send(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
    socket: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    CHAIN.send(fn_ptr, socket, buf, len, flags)
}

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
pub extern "C" fn writev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    CHAIN.writev(fn_ptr, fd, iov, iovcnt)
}

#[must_use]
pub extern "C" fn pwritev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    CHAIN.pwritev(fn_ptr, fd, iov, iovcnt, offset)
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

/// socket

#[cfg(target_os = "linux")]
#[must_use]
pub extern "C" fn accept4(
    fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int>,
    fd: c_int,
    addr: *mut sockaddr,
    len: *mut socklen_t,
    flg: c_int,
) -> c_int {
    CHAIN.accept4(fn_ptr, fd, addr, len, flg)
}
