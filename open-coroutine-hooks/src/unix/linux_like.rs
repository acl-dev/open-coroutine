use libc::{c_int, sockaddr, socklen_t};
use once_cell::sync::Lazy;

static ACCEPT4: Lazy<extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int> =
    init_hook!("accept4");

#[no_mangle]
pub extern "C" fn accept4(
    fd: c_int,
    addr: *mut sockaddr,
    len: *mut socklen_t,
    flg: c_int,
) -> c_int {
    open_coroutine_core::unbreakable!(
        impl_read_hook!((Lazy::force(&ACCEPT4))(fd, addr, len, flg)),
        "accept4"
    )
}
