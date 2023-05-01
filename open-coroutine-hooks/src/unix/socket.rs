use libc::{c_int, sockaddr, socklen_t};
use once_cell::sync::Lazy;

static CONNECT: Lazy<extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int> =
    init_hook!("connect");

#[no_mangle]
pub extern "C" fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int {
    open_coroutine_core::unbreakable!((Lazy::force(&CONNECT))(socket, address, len), "connect")
}

static LISTEN: Lazy<extern "C" fn(c_int, c_int) -> c_int> = init_hook!("listen");

#[no_mangle]
pub extern "C" fn listen(socket: c_int, backlog: c_int) -> c_int {
    //unnecessary non blocking impl for listen
    open_coroutine_core::unbreakable!((Lazy::force(&LISTEN))(socket, backlog), "listen")
}
