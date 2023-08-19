use libc::{c_int, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

static RECV: Lazy<extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t> = init_hook!("recv");

#[no_mangle]
pub extern "C" fn recv(socket: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_read_hook!((Lazy::force(&RECV))(socket, buf, len, flags)),
        "recv"
    )
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
    open_coroutine_core::unbreakable!(
        impl_expected_read_hook!((Lazy::force(&RECVFROM))(
            socket, buf, len, flags, addr, addrlen
        )),
        "recvfrom"
    )
}

static PREAD: Lazy<extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t> =
    init_hook!("pread");

#[no_mangle]
pub extern "C" fn pread(fd: c_int, buf: *mut c_void, count: size_t, offset: off_t) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_read_hook!((Lazy::force(&PREAD))(fd, buf, count, offset)),
        "pread"
    )
}
