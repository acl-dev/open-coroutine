use libc::{iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};

static RECV: Lazy<extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t> = init_hook!("recv");

#[no_mangle]
pub extern "C" fn recv(socket: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::syscall::recv(Some(Lazy::force(&RECV)), socket, buf, len, flags)
}

static RECVFROM: Lazy<
    extern "C" fn(c_int, *mut c_void, size_t, c_int, *mut sockaddr, *mut socklen_t) -> ssize_t,
> = init_hook!("recvfrom");

#[no_mangle]
pub extern "C" fn recvfrom(
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> ssize_t {
    open_coroutine_core::syscall::recvfrom(
        Some(Lazy::force(&RECVFROM)),
        socket,
        buf,
        len,
        flags,
        addr,
        addrlen,
    )
}

static PREAD: Lazy<extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t> =
    init_hook!("pread");

#[no_mangle]
pub extern "C" fn pread(fd: c_int, buf: *mut c_void, count: size_t, offset: off_t) -> ssize_t {
    open_coroutine_core::syscall::pread(Some(Lazy::force(&PREAD)), fd, buf, count, offset)
}

static READV: Lazy<extern "C" fn(c_int, *const iovec, c_int) -> ssize_t> = init_hook!("readv");

#[no_mangle]
pub extern "C" fn readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    open_coroutine_core::syscall::readv(Some(Lazy::force(&READV)), fd, iov, iovcnt)
}

static PREADV: Lazy<extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t> =
    init_hook!("preadv");

#[no_mangle]
pub extern "C" fn preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t {
    open_coroutine_core::syscall::preadv(Some(Lazy::force(&PREADV)), fd, iov, iovcnt, offset)
}

static RECVMSG: Lazy<extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t> = init_hook!("recvmsg");

#[no_mangle]
pub extern "C" fn recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t {
    open_coroutine_core::syscall::recvmsg(Some(Lazy::force(&RECVMSG)), fd, msg, flags)
}
