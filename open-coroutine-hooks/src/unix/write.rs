use libc::{c_int, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
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

static WRITEV: Lazy<extern "C" fn(c_int, *const iovec, c_int) -> ssize_t> = init_hook!("writev");

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
#[no_mangle]
pub extern "C" fn writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_batch_write_hook!((Lazy::force(&WRITEV))(fd, iov, iovcnt,)),
        "writev"
    )
}

static PWRITEV: Lazy<extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t> =
    init_hook!("pwritev");

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
#[no_mangle]
pub extern "C" fn pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_batch_write_hook!((Lazy::force(&PWRITEV))(fd, iov, iovcnt, offset)),
        "pwritev"
    )
}

static SENDMSG: Lazy<extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t> = init_hook!("sendmsg");

#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    trivial_numeric_casts
)]
#[no_mangle]
pub extern "C" fn sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        {
            let blocking = crate::unix::is_blocking(fd);
            if blocking {
                crate::unix::set_non_blocking(fd);
            }
            let msghdr = unsafe { *msg };
            let vec = unsafe {
                Vec::from_raw_parts(
                    msghdr.msg_iov,
                    msghdr.msg_iovlen as usize,
                    msghdr.msg_iovlen as usize,
                )
            };
            let mut total_sent = 0;
            for iovec in vec {
                let length = iovec.iov_len;
                let mut sent = 0;
                let mut r = 0;
                while sent < length {
                    let mut inner_iovec = iovec {
                        iov_base: (iovec.iov_base as usize + sent) as *mut c_void,
                        iov_len: length - sent,
                    };
                    let inner_msghdr = msghdr {
                        msg_name: msghdr.msg_name,
                        msg_namelen: msghdr.msg_namelen,
                        msg_iov: &mut inner_iovec,
                        msg_iovlen: 1,
                        msg_control: msghdr.msg_control,
                        msg_controllen: msghdr.msg_controllen,
                        msg_flags: msghdr.msg_flags,
                    };
                    r = (Lazy::force(&SENDMSG))(fd, &inner_msghdr, flags);
                    if r != -1 {
                        crate::unix::reset_errno();
                        sent += r as size_t;
                        if sent >= length {
                            r = sent as ssize_t;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait write event
                        if open_coroutine_core::event_loop::EventLoops::wait_write_event(
                            fd,
                            Some(std::time::Duration::from_millis(10)),
                        )
                        .is_err()
                        {
                            return -1;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        return -1;
                    }
                }
                total_sent += r;
            }
            if blocking {
                crate::unix::set_blocking(fd);
            }
            total_sent
        },
        "sendmsg"
    )
}
