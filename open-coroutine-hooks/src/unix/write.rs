use libc::{c_int, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

macro_rules! impl_expected_write_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < $length {
            r = $fn(
                $socket,
                ($buffer as usize + sent) as *const c_void,
                $length - sent,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::reset_errno();
                sent += r as size_t;
                if sent >= $length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if open_coroutine_core::event_loop::EventLoops::wait_write_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

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
