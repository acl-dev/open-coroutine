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
            #[cfg(target_os = "linux")]
            if open_coroutine_iouring::version::support_io_uring() {
                return open_coroutine_core::event_loop::EventLoops::connect(
                    Some(Lazy::force(&CONNECT)),
                    socket,
                    address,
                    len,
                );
            }
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
                        r = -1;
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
            if r == -1 && Error::last_os_error().raw_os_error() == Some(libc::ETIMEDOUT) {
                crate::unix::set_errno(libc::EINPROGRESS);
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

static ACCEPT: Lazy<extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int> =
    init_hook!("accept");

#[no_mangle]
pub extern "C" fn accept(
    socket: c_int,
    address: *mut sockaddr,
    address_len: *mut socklen_t,
) -> c_int {
    open_coroutine_core::unbreakable!(
        impl_read_hook!((Lazy::force(&ACCEPT))(socket, address, address_len)),
        "accept"
    )
}

static SHUTDOWN: Lazy<extern "C" fn(c_int, c_int) -> c_int> = init_hook!("shutdown");

#[no_mangle]
pub extern "C" fn shutdown(socket: c_int, how: c_int) -> c_int {
    open_coroutine_core::unbreakable!(
        {
            //取消对fd的监听
            match how {
                libc::SHUT_RD => EventLoops::del_read_event(socket),
                libc::SHUT_WR => EventLoops::del_write_event(socket),
                libc::SHUT_RDWR => EventLoops::del_event(socket),
                _ => {
                    crate::unix::set_errno(libc::EINVAL);
                    return -1;
                }
            };
            (Lazy::force(&SHUTDOWN))(socket, how)
        },
        "shutdown"
    )
}
