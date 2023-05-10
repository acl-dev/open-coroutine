use libc::{c_int, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

macro_rules! impl_expected_read_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut received = 0;
        let mut r = 0;
        while received < $length {
            r = $fn(
                $socket,
                ($buffer as usize + received) as *mut c_void,
                $length - received,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::reset_errno();
                received += r as size_t;
                if received >= $length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if open_coroutine_core::event_loop::EventLoops::wait_read_event(
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
