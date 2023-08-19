use libc::{c_int, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

static SEND: Lazy<extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t> =
    init_hook!("send");

#[no_mangle]
pub extern "C" fn send(socket: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_write_hook!((Lazy::force(&SEND))(socket, buf, len, flags)),
        "send"
    )
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
    open_coroutine_core::unbreakable!(
        impl_expected_write_hook!((Lazy::force(&SENDTO))(
            socket, buf, len, flags, addr, addrlen
        )),
        "sendto"
    )
}

static PWRITE: Lazy<extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t> =
    init_hook!("pwrite");

#[no_mangle]
pub extern "C" fn pwrite(fd: c_int, buf: *const c_void, count: size_t, offset: off_t) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_write_hook!((Lazy::force(&PWRITE))(fd, buf, count, offset)),
        "pwrite"
    )
}
