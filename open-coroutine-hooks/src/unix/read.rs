use libc::{c_int, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

static RECV: Lazy<extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t> = init_hook!("recv");

#[no_mangle]
pub extern "C" fn recv(socket: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        {
            #[cfg(target_os = "linux")]
            if open_coroutine_iouring::version::support_io_uring() {
                return open_coroutine_core::event_loop::EventLoops::recv(
                    Some(Lazy::force(&RECV)),
                    socket,
                    buf,
                    len,
                    flags,
                );
            }
            impl_expected_read_hook!((Lazy::force(&RECV))(socket, buf, len, flags))
        },
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

static READV: Lazy<extern "C" fn(c_int, *const iovec, c_int) -> ssize_t> = init_hook!("readv");

#[no_mangle]
pub extern "C" fn readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_batch_read_hook!((Lazy::force(&READV))(fd, iov, iovcnt,)),
        "readv"
    )
}

static PREADV: Lazy<extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t> =
    init_hook!("preadv");

#[no_mangle]
pub extern "C" fn preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t {
    open_coroutine_core::unbreakable!(
        impl_expected_batch_read_hook!((Lazy::force(&PREADV))(fd, iov, iovcnt, offset)),
        "preadv"
    )
}

static RECVMSG: Lazy<extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t> = init_hook!("recvmsg");

#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unnecessary_cast,
    trivial_numeric_casts
)]
#[no_mangle]
pub extern "C" fn recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t {
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
            let mut received = 0;
            let mut r = 0;
            while received < length {
                // find from-index
                let mut from_index = 0;
                for (i, v) in pices.iter().enumerate() {
                    if received < *v {
                        from_index = i;
                        break;
                    }
                }
                // calculate offset
                let current_received_offset = if from_index > 0 {
                    received.saturating_sub(pices[from_index.saturating_sub(1)])
                } else {
                    received
                };
                // remove already received
                for _ in 0..from_index {
                    _ = vec.pop_front();
                    _ = pices.pop_front();
                }
                // build syscall args
                vec[0] = iovec {
                    iov_base: (vec[0].iov_base as usize + current_received_offset) as *mut c_void,
                    iov_len: vec[0].iov_len - current_received_offset,
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
                let mut new_msg = msghdr {
                    msg_name: msghdr.msg_name,
                    msg_namelen: msghdr.msg_namelen,
                    msg_iov: vec.get_mut(0).unwrap(),
                    msg_iovlen: len,
                    msg_control: msghdr.msg_control,
                    msg_controllen: msghdr.msg_controllen,
                    msg_flags: msghdr.msg_flags,
                };
                r = (Lazy::force(&RECVMSG))(fd, &mut new_msg, flags);
                if r != -1 {
                    crate::unix::reset_errno();
                    received += r as usize;
                    if received >= length || r == 0 {
                        r = received as ssize_t;
                        break;
                    }
                }
                let error_kind = std::io::Error::last_os_error().kind();
                if error_kind == std::io::ErrorKind::WouldBlock {
                    //wait read event
                    if open_coroutine_core::event_loop::EventLoops::wait_read_event(
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
        "recvmsg"
    )
}
