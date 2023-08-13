use libc::{c_int, sockaddr, socklen_t};
use once_cell::sync::Lazy;
use open_coroutine_core::event_loop::EventLoops;
use std::ffi::c_void;
use std::io::Error;

static CONNECT: Lazy<extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int> =
    init_hook!("connect");

#[no_mangle]
pub extern "C" fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int {
    open_coroutine_core::unbreakable!(
        {
            let blocking = crate::unix::is_blocking(socket);
            if blocking {
                crate::unix::set_non_blocking(socket);
            }
            let mut r;
            loop {
                r = (Lazy::force(&CONNECT))(socket, address, len);
                if r == 0 {
                    crate::unix::reset_errno();
                    break;
                }
                let errno = Error::last_os_error().raw_os_error();
                if errno == Some(libc::EINPROGRESS) {
                    //阻塞，直到写事件发生
                    if EventLoops::wait_write_event(
                        socket,
                        Some(std::time::Duration::from_millis(10)),
                    )
                    .is_err()
                    {
                        r = -1;
                        break;
                    }
                    let mut err: c_int = 0;
                    unsafe {
                        let mut len: socklen_t = std::mem::zeroed();
                        r = libc::getsockopt(
                            socket,
                            libc::SOL_SOCKET,
                            libc::SO_ERROR,
                            (std::ptr::addr_of_mut!(err)).cast::<c_void>(),
                            &mut len,
                        );
                    }
                    if r != 0 {
                        break;
                    }
                    if err == 0 {
                        crate::unix::reset_errno();
                        r = 0;
                        break;
                    };
                    crate::unix::set_errno(err);
                    r = -1;
                    break;
                } else if errno != Some(libc::EINTR) {
                    r = -1;
                    break;
                }
            }
            if blocking {
                crate::unix::set_blocking(socket);
            }
            r
        },
        "connect"
    )
}

static LISTEN: Lazy<extern "C" fn(c_int, c_int) -> c_int> = init_hook!("listen");

#[no_mangle]
pub extern "C" fn listen(socket: c_int, backlog: c_int) -> c_int {
    //unnecessary non blocking impl for listen
    open_coroutine_core::unbreakable!((Lazy::force(&LISTEN))(socket, backlog), "listen")
}
