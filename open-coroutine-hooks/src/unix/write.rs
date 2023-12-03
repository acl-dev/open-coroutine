use libc::{iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

static SEND: Lazy<extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t> =
    init_hook!("send");

#[no_mangle]
pub extern "C" fn send(socket: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::syscall::send(Some(Lazy::force(&SEND)), socket, buf, len, flags)
}

static SENDTO: Lazy<
    extern "C" fn(c_int, *const c_void, size_t, c_int, *const sockaddr, socklen_t) -> ssize_t,
> = init_hook!("sendto");

#[no_mangle]
pub extern "C" fn sendto(
    socket: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> ssize_t {
    open_coroutine_core::syscall::sendto(
        Some(Lazy::force(&SENDTO)),
        socket,
        buf,
        len,
        flags,
        addr,
        addrlen,
    )
}

static PWRITE: Lazy<extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t> =
    init_hook!("pwrite");

#[no_mangle]
pub extern "C" fn pwrite(fd: c_int, buf: *const c_void, count: size_t, offset: off_t) -> ssize_t {
    open_coroutine_core::syscall::pwrite(Some(Lazy::force(&PWRITE)), fd, buf, count, offset)
}

static WRITEV: Lazy<extern "C" fn(c_int, *const iovec, c_int) -> ssize_t> = init_hook!("writev");

#[no_mangle]
pub extern "C" fn writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    open_coroutine_core::syscall::writev(Some(Lazy::force(&WRITEV)), fd, iov, iovcnt)
}

static PWRITEV: Lazy<extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t> =
    init_hook!("pwritev");

#[no_mangle]
pub extern "C" fn pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t {
    open_coroutine_core::syscall::pwritev(Some(Lazy::force(&PWRITEV)), fd, iov, iovcnt, offset)
}

static SENDMSG: Lazy<extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t> = init_hook!("sendmsg");

#[no_mangle]
pub extern "C" fn sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t {
    open_coroutine_core::syscall::sendmsg(Some(Lazy::force(&SENDMSG)), fd, msg, flags)
}
