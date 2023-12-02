use libc::{sockaddr, socklen_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

static SOCKET: Lazy<extern "C" fn(c_int, c_int, c_int) -> c_int> = init_hook!("socket");

#[no_mangle]
pub extern "C" fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int {
    open_coroutine_core::syscall::socket(Some(Lazy::force(&SOCKET)), domain, ty, protocol)
}

static CONNECT: Lazy<extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int> =
    init_hook!("connect");

#[no_mangle]
pub extern "C" fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int {
    open_coroutine_core::syscall::connect(Some(Lazy::force(&CONNECT)), socket, address, len)
}

static LISTEN: Lazy<extern "C" fn(c_int, c_int) -> c_int> = init_hook!("listen");

#[no_mangle]
pub extern "C" fn listen(socket: c_int, backlog: c_int) -> c_int {
    open_coroutine_core::syscall::listen(Some(Lazy::force(&LISTEN)), socket, backlog)
}

static ACCEPT: Lazy<extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int> =
    init_hook!("accept");

#[no_mangle]
pub extern "C" fn accept(
    socket: c_int,
    address: *mut sockaddr,
    address_len: *mut socklen_t,
) -> c_int {
    open_coroutine_core::syscall::accept(Some(Lazy::force(&ACCEPT)), socket, address, address_len)
}

static SHUTDOWN: Lazy<extern "C" fn(c_int, c_int) -> c_int> = init_hook!("shutdown");

#[no_mangle]
pub extern "C" fn shutdown(socket: c_int, how: c_int) -> c_int {
    open_coroutine_core::syscall::shutdown(Some(Lazy::force(&SHUTDOWN)), socket, how)
}
