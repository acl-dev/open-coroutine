use libc::{c_int, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

static SEND: Lazy<extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t> =
    init_hook!("send");

#[no_mangle]
pub extern "C" fn send(socket: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        {
            #[cfg(target_os = "linux")]
            if open_coroutine_iouring::version::support_io_uring() {
                return open_coroutine_core::event_loop::EventLoops::send(
                    Some(Lazy::force(&SEND)),
                    socket,
                    buf,
                    len,
                    flags,
                );
            }
            impl_expected_write_hook!((Lazy::force(&SEND))(socket, buf, len, flags))
        },
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
    clippy::unnecessary_cast,
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
            let mut vec = std::collections::VecDeque::from(unsafe {
                Vec::from_raw_parts(
                    msghdr.msg_iov,
                    msghdr.msg_iovlen as usize,
                    msghdr.msg_iovlen as usize,
                )
            });
            let mut length = 0;
            let mut pices = std::collections::VecDeque::new();
            for iovec in &vec {
                length += iovec.iov_len;
                pices.push_back(length);
            }
            let mut sent = 0;
            let mut r = 0;
            while sent < length {
                // find from-index
                let mut from_index = 0;
                for (i, v) in pices.iter().enumerate() {
                    if sent < *v {
                        from_index = i;
                        break;
                    }
                }
                // calculate offset
                let current_sent_offset = if from_index > 0 {
                    sent.saturating_sub(pices[from_index.saturating_sub(1)])
                } else {
                    sent
                };
                // remove already sent
                for _ in 0..from_index {
                    _ = vec.pop_front();
                    _ = pices.pop_front();
                }
                // build syscall args
                vec[0] = iovec {
                    iov_base: (vec[0].iov_base as usize + current_sent_offset) as *mut c_void,
                    iov_len: vec[0].iov_len - current_sent_offset,
                };
                cfg_if::cfg_if! {
                    if #[cfg(any(
                        target_os = "linux",
                        target_os = "l4re",
                        target_os = "android",
                        target_os = "emscripten"
                    ))] {
                        let len = vec.len();
                    } else {
                        let len = c_int::try_from(vec.len()).unwrap();
                    }
                }
                let new_msg = msghdr {
                    msg_name: msghdr.msg_name,
                    msg_namelen: msghdr.msg_namelen,
                    msg_iov: vec.get_mut(0).unwrap(),
                    msg_iovlen: len,
                    msg_control: msghdr.msg_control,
                    msg_controllen: msghdr.msg_controllen,
                    msg_flags: msghdr.msg_flags,
                };
                r = (Lazy::force(&SENDMSG))(fd, &new_msg, flags);
                if r != -1 {
                    crate::unix::reset_errno();
                    sent += r as usize;
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
                        break;
                    }
                } else if error_kind != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
            if blocking {
                crate::unix::set_blocking(fd);
            }
            r
        },
        "sendmsg"
    )
}
