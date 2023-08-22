use libc::{c_int, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::c_void;

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

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
#[no_mangle]
pub extern "C" fn recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t {
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
            let mut total_received = 0;
            for iovec in vec {
                let length = iovec.iov_len;
                let mut received = 0;
                let mut r = 0;
                while received < length {
                    let mut inner_iovec = iovec {
                        iov_base: (iovec.iov_base as usize + received) as *mut c_void,
                        iov_len: length - received,
                    };
                    let mut inner_msghdr = msghdr {
                        msg_name: msghdr.msg_name,
                        msg_namelen: msghdr.msg_namelen,
                        msg_iov: &mut inner_iovec,
                        msg_iovlen: 1,
                        msg_control: msghdr.msg_control,
                        msg_controllen: msghdr.msg_controllen,
                        msg_flags: msghdr.msg_flags,
                    };
                    r = (Lazy::force(&RECVMSG))(fd, &mut inner_msghdr, flags);
                    if r != -1 {
                        crate::unix::reset_errno();
                        received += r as size_t;
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
                            return -1;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        return -1;
                    }
                }
                total_received += r;
            }
            if blocking {
                crate::unix::set_blocking(fd);
            }
            total_received
        },
        "recvmsg"
    )
}
