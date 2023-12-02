use crate::common::{Current, Named};
use crate::coroutine::StateCoroutine;
use crate::net::event_loop::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_errno, set_non_blocking};
use crate::syscall::raw::RawLinuxSyscall;
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
use crate::{impl_expected_batch_read_hook, impl_expected_read_hook, impl_read_hook};
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timespec,
    timeval,
};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_uint, c_void};
use std::time::Duration;

static CHAIN: Lazy<RawLinuxSyscall> = Lazy::new(RawLinuxSyscall::default);

/// sleep

#[must_use]
pub extern "C" fn sleep(_: Option<&extern "C" fn(c_uint) -> c_uint>, secs: c_uint) -> c_uint {
    crate::info!("sleep hooked");
    _ = EventLoops::wait_event(Some(Duration::from_secs(u64::from(secs))));
    reset_errno();
    0
}

#[must_use]
pub extern "C" fn usleep(
    _: Option<&extern "C" fn(c_uint) -> c_int>,
    microseconds: c_uint,
) -> c_int {
    crate::info!("usleep hooked");
    let time = match u64::from(microseconds).checked_mul(1_000) {
        Some(v) => Duration::from_nanos(v),
        None => Duration::MAX,
    };
    _ = EventLoops::wait_event(Some(time));
    reset_errno();
    0
}

#[must_use]
pub extern "C" fn nanosleep(
    _: Option<&extern "C" fn(*const timespec, *mut timespec) -> c_int>,
    rqtp: *const timespec,
    rmtp: *mut timespec,
) -> c_int {
    crate::info!("nanosleep hooked");
    let rqtp = unsafe { *rqtp };
    if rqtp.tv_sec < 0 || rqtp.tv_nsec < 0 || rqtp.tv_nsec > 999_999_999 {
        set_errno(libc::EINVAL);
        return -1;
    }
    //等待事件到来
    _ = EventLoops::wait_event(Some(Duration::new(rqtp.tv_sec as u64, rqtp.tv_nsec as u32)));
    reset_errno();
    if !rmtp.is_null() {
        unsafe {
            (*rmtp).tv_sec = 0;
            (*rmtp).tv_nsec = 0;
        }
    }
    0
}

/// poll

#[must_use]
pub extern "C" fn poll(
    fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
    fds: *mut pollfd,
    nfds: nfds_t,
    timeout: c_int,
) -> c_int {
    unbreakable!(
        {
            let mut t = if timeout < 0 { c_int::MAX } else { timeout };
            let mut x = 1;
            let mut r;
            // just check select every x ms
            loop {
                r = CHAIN.poll(fn_ptr, fds, nfds, 0);
                if r != 0 || t == 0 {
                    break;
                }
                _ = EventLoops::wait_event(Some(Duration::from_millis(t.min(x) as u64)));
                if t != c_int::MAX {
                    t = if t > x { t - x } else { 0 };
                }
                if x < 16 {
                    x <<= 1;
                }
            }
            r
        },
        poll
    )
}

#[must_use]
pub extern "C" fn select(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
    >,
    nfds: c_int,
    readfds: *mut fd_set,
    writefds: *mut fd_set,
    errorfds: *mut fd_set,
    timeout: *mut timeval,
) -> c_int {
    unbreakable!(
        {
            let mut t = if timeout.is_null() {
                c_uint::MAX
            } else {
                unsafe { ((*timeout).tv_sec as c_uint) * 1_000_000 + (*timeout).tv_usec as c_uint }
            };
            let mut o = timeval {
                tv_sec: 0,
                tv_usec: 0,
            };
            let mut s: [fd_set; 3] = unsafe { std::mem::zeroed() };
            unsafe {
                if !readfds.is_null() {
                    s[0] = *readfds;
                }
                if !writefds.is_null() {
                    s[1] = *writefds;
                }
                if !errorfds.is_null() {
                    s[2] = *errorfds;
                }
            }
            let mut x = 1;
            let mut r;
            // just check poll every x ms
            loop {
                r = CHAIN.select(fn_ptr, nfds, readfds, writefds, errorfds, &mut o);
                if r != 0 || t == 0 {
                    break;
                }
                _ = EventLoops::wait_event(Some(Duration::from_millis(u64::from(t.min(x)))));
                if t != c_uint::MAX {
                    t = if t > x { t - x } else { 0 };
                }
                if x < 16 {
                    x <<= 1;
                }
                unsafe {
                    if !readfds.is_null() {
                        *readfds = s[0];
                    }
                    if !writefds.is_null() {
                        *writefds = s[1];
                    }
                    if !errorfds.is_null() {
                        *errorfds = s[2];
                    }
                }
                o.tv_sec = 0;
                o.tv_usec = 0;
            }
            r
        },
        select
    )
}

/// socket

#[must_use]
pub extern "C" fn socket(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
    domain: c_int,
    ty: c_int,
    protocol: c_int,
) -> c_int {
    unbreakable!(CHAIN.socket(fn_ptr, domain, ty, protocol), socket)
}

#[must_use]
pub extern "C" fn listen(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    backlog: c_int,
) -> c_int {
    unbreakable!(CHAIN.listen(fn_ptr, socket, backlog), listen)
}

#[must_use]
pub extern "C" fn accept(
    fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
    socket: c_int,
    address: *mut sockaddr,
    address_len: *mut socklen_t,
) -> c_int {
    unbreakable!(
        impl_read_hook!(CHAIN, accept, fn_ptr, socket, address, address_len),
        accept
    )
}

#[must_use]
pub extern "C" fn connect(
    fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
    socket: c_int,
    address: *const sockaddr,
    len: socklen_t,
) -> c_int {
    unbreakable!(
        {
            #[cfg(target_os = "linux")]
            if open_coroutine_iouring::version::support_io_uring() {
                return crate::net::event_loop::EventLoops::connect(fn_ptr, socket, address, len);
            }
            let blocking = is_blocking(socket);
            if blocking {
                set_non_blocking(socket);
            }
            let mut r;
            loop {
                r = CHAIN.connect(fn_ptr, socket, address, len);
                if r == 0 {
                    reset_errno();
                    break;
                }
                let errno = std::io::Error::last_os_error().raw_os_error();
                if errno == Some(libc::EINPROGRESS) {
                    //阻塞，直到写事件发生
                    if EventLoops::wait_write_event(socket, Some(Duration::from_millis(10)))
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
                    if err != 0 {
                        set_errno(err);
                        r = -1;
                        break;
                    };
                    unsafe {
                        let mut address = std::mem::zeroed();
                        let mut address_len = std::mem::zeroed();
                        r = libc::getpeername(socket, &mut address, &mut address_len);
                    }
                    if r == 0 {
                        reset_errno();
                        r = 0;
                        break;
                    }
                } else if errno != Some(libc::EINTR) {
                    r = -1;
                    break;
                }
            }
            if blocking {
                set_blocking(socket);
            }
            if r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ETIMEDOUT) {
                set_errno(libc::EINPROGRESS);
            }
            r
        },
        connect
    )
}

#[must_use]
pub extern "C" fn shutdown(
    fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
    socket: c_int,
    how: c_int,
) -> c_int {
    unbreakable!(
        {
            //取消对fd的监听
            match how {
                libc::SHUT_RD => EventLoops::del_read_event(socket),
                libc::SHUT_WR => EventLoops::del_write_event(socket),
                libc::SHUT_RDWR => EventLoops::del_event(socket),
                _ => {
                    set_errno(libc::EINVAL);
                    return -1;
                }
            };
            CHAIN.shutdown(fn_ptr, socket, how)
        },
        shutdown
    )
}

#[must_use]
pub extern "C" fn close(fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
    unbreakable!(CHAIN.close(fn_ptr, fd), close)
}

/// read

#[must_use]
pub extern "C" fn recv(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    unbreakable!(
        {
            #[cfg(target_os = "linux")]
            if open_coroutine_iouring::version::support_io_uring() {
                return crate::net::event_loop::EventLoops::recv(fn_ptr, socket, buf, len, flags);
            }
            impl_expected_read_hook!(CHAIN, recv, fn_ptr, socket, buf, len, flags)
        },
        recv
    )
}

#[must_use]
pub extern "C" fn recvfrom(
    fn_ptr: Option<
        &extern "C" fn(c_int, *mut c_void, size_t, c_int, *mut sockaddr, *mut socklen_t) -> ssize_t,
    >,
    socket: c_int,
    buf: *mut c_void,
    len: size_t,
    flags: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> ssize_t {
    unbreakable!(
        impl_expected_read_hook!(CHAIN, recvfrom, fn_ptr, socket, buf, len, flags, addr, addrlen),
        recvfrom
    )
}

#[must_use]
pub extern "C" fn read(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    count: size_t,
) -> ssize_t {
    unbreakable!(
        impl_expected_read_hook!(CHAIN, read, fn_ptr, fd, buf, count,),
        read
    )
}

#[must_use]
pub extern "C" fn pread(
    fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *mut c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    unbreakable!(
        impl_expected_read_hook!(CHAIN, pread, fn_ptr, fd, buf, count, offset),
        pread
    )
}

#[must_use]
pub extern "C" fn readv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    unbreakable!(
        impl_expected_batch_read_hook!(CHAIN, readv, fn_ptr, fd, iov, iovcnt,),
        readv
    )
}

#[must_use]
pub extern "C" fn preadv(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    unbreakable!(
        impl_expected_batch_read_hook!(CHAIN, preadv, fn_ptr, fd, iov, iovcnt, offset),
        preadv
    )
}

#[must_use]
pub extern "C" fn recvmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *mut msghdr,
    flags: c_int,
) -> ssize_t {
    unbreakable!(
        {
            let blocking = is_blocking(fd);
            if blocking {
                set_non_blocking(fd);
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
                r = CHAIN.recvmsg(fn_ptr, fd, &mut new_msg, flags);
                if r != -1 {
                    reset_errno();
                    received += r as usize;
                    if received >= length || r == 0 {
                        r = received as ssize_t;
                        break;
                    }
                }
                let error_kind = std::io::Error::last_os_error().kind();
                if error_kind == std::io::ErrorKind::WouldBlock {
                    //wait read event
                    if EventLoops::wait_read_event(fd, Some(Duration::from_millis(10))).is_err() {
                        break;
                    }
                } else if error_kind != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
            if blocking {
                set_blocking(fd);
            }
            r
        },
        recvmsg
    )
}

/// write

#[must_use]
pub extern "C" fn send(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
    socket: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
) -> ssize_t {
    CHAIN.send(fn_ptr, socket, buf, len, flags)
}

#[must_use]
pub extern "C" fn sendto(
    fn_ptr: Option<
        &extern "C" fn(c_int, *const c_void, size_t, c_int, *const sockaddr, socklen_t) -> ssize_t,
    >,
    socket: c_int,
    buf: *const c_void,
    len: size_t,
    flags: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> ssize_t {
    CHAIN.sendto(fn_ptr, socket, buf, len, flags, addr, addrlen)
}

#[must_use]
pub extern "C" fn write(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    count: size_t,
) -> ssize_t {
    CHAIN.write(fn_ptr, fd, buf, count)
}

#[must_use]
pub extern "C" fn pwrite(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    count: size_t,
    offset: off_t,
) -> ssize_t {
    CHAIN.pwrite(fn_ptr, fd, buf, count, offset)
}

#[must_use]
pub extern "C" fn writev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
) -> ssize_t {
    CHAIN.writev(fn_ptr, fd, iov, iovcnt)
}

#[must_use]
pub extern "C" fn pwritev(
    fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
    fd: c_int,
    iov: *const iovec,
    iovcnt: c_int,
    offset: off_t,
) -> ssize_t {
    CHAIN.pwritev(fn_ptr, fd, iov, iovcnt, offset)
}

#[must_use]
pub extern "C" fn sendmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *const msghdr,
    flags: c_int,
) -> ssize_t {
    CHAIN.sendmsg(fn_ptr, fd, msg, flags)
}

/// poll

#[cfg(target_os = "linux")]
#[must_use]
pub extern "C" fn epoll_ctl(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
    epfd: c_int,
    op: c_int,
    fd: c_int,
    event: *mut epoll_event,
) -> c_int {
    CHAIN.epoll_ctl(fn_ptr, epfd, op, fd, event)
}

/// socket

#[cfg(target_os = "linux")]
#[must_use]
pub extern "C" fn accept4(
    fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int>,
    fd: c_int,
    addr: *mut sockaddr,
    len: *mut socklen_t,
    flg: c_int,
) -> c_int {
    unbreakable!(
        impl_read_hook!(CHAIN, accept4, fn_ptr, fd, addr, len, flg),
        accept4
    )
}
