/**
do not impl close/read hook !!!!!
it will not pass linux CI !!!!!
 */
use crate::unix::common::*;
use once_cell::sync::Lazy;
use std::ffi::c_void;

#[macro_use]
mod common;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
mod bsd;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
mod linux_like;

//sleep相关
#[no_mangle]
pub extern "C" fn sleep(secs: libc::c_uint) -> libc::c_uint {
    let rqtp = libc::timespec {
        tv_sec: secs as i64,
        tv_nsec: 0,
    };
    let mut rmtp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    nanosleep(&rqtp, &mut rmtp);
    rmtp.tv_sec as u32
}

#[no_mangle]
pub extern "C" fn usleep(secs: libc::c_uint) -> libc::c_int {
    let secs = secs as i64;
    let sec = secs / 1_000_000;
    let nsec = (secs % 1_000_000) * 1000;
    let rqtp = libc::timespec {
        tv_sec: sec,
        tv_nsec: nsec,
    };
    let mut rmtp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    nanosleep(&rqtp, &mut rmtp)
}

static NANOSLEEP: Lazy<extern "C" fn(*const libc::timespec, *mut libc::timespec) -> libc::c_int> =
    init_hook!("nanosleep");

#[no_mangle]
pub extern "C" fn nanosleep(rqtp: *const libc::timespec, rmtp: *mut libc::timespec) -> libc::c_int {
    let mut rqtp = unsafe { *rqtp };
    if rqtp.tv_sec < 0 || rqtp.tv_nsec < 0 {
        return -1;
    }
    let nanos_time = match (rqtp.tv_sec as u64).checked_mul(1_000_000_000) {
        Some(v) => v.checked_add(rqtp.tv_nsec as u64).unwrap_or(u64::MAX),
        None => u64::MAX,
    };
    let timeout_time = timer_utils::add_timeout_time(nanos_time);
    loop {
        let _ = base_coroutine::EventLoop::round_robin_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        let schedule_finished_time = timer_utils::now();
        let left_time = match timeout_time.checked_sub(schedule_finished_time) {
            Some(v) => v,
            None => {
                if !rmtp.is_null() {
                    unsafe {
                        (*rmtp).tv_sec = 0;
                        (*rmtp).tv_nsec = 0;
                    }
                }
                return 0;
            }
        } as i64;
        let sec = left_time / 1_000_000_000;
        let nsec = left_time % 1_000_000_000;
        rqtp = libc::timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        };
        //注意这里获取的是原始系统函数nanosleep的指针
        //相当于libc::nanosleep(&rqtp, rmtp)
        if (Lazy::force(&NANOSLEEP))(&rqtp, rmtp) == 0 {
            reset_errno();
            return 0;
        }
    }
}

//socket相关
static CONNECT: Lazy<
    extern "C" fn(libc::c_int, *const libc::sockaddr, libc::socklen_t) -> libc::c_int,
> = init_hook!("connect");

#[no_mangle]
pub extern "C" fn connect(
    socket: libc::c_int,
    address: *const libc::sockaddr,
    len: libc::socklen_t,
) -> libc::c_int {
    let blocking = is_blocking(socket);
    //阻塞，epoll_wait/kevent等待直到写事件
    if blocking {
        set_non_blocking(socket, true);
    }
    let event_loop = base_coroutine::EventLoop::next();
    let mut r;
    loop {
        r = (Lazy::force(&CONNECT))(socket, address, len);
        if r == 0 {
            reset_errno();
            break;
        }
        let errno = std::io::Error::last_os_error().raw_os_error();
        if errno == Some(libc::EINPROGRESS) {
            //等待写事件
            if let Err(e) = event_loop.wait_write_event(socket, None) {
                match e.kind() {
                    //maybe invoke by Monitor::signal(), just ignore this
                    std::io::ErrorKind::Interrupted => reset_errno(),
                    _ => {
                        r = -1;
                        break;
                    }
                }
            }
            unsafe {
                let mut len: libc::socklen_t = std::mem::zeroed();
                let mut err: libc::c_int = 0;
                r = libc::getsockopt(
                    socket,
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    &mut err as *mut _ as *mut c_void,
                    &mut len,
                );
                if r != 0 {
                    break;
                }
                if err == 0 {
                    reset_errno();
                    r = 0;
                    break;
                };
                set_errno(err);
            }
            r = -1;
            break;
        } else if errno != Some(libc::EINTR) {
            r = -1;
            break;
        }
    }
    if blocking {
        set_non_blocking(socket, false);
    }
    r
}

static LISTEN: Lazy<extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int> = init_hook!("listen");

#[no_mangle]
pub extern "C" fn listen(socket: libc::c_int, backlog: libc::c_int) -> libc::c_int {
    let _ = base_coroutine::EventLoop::round_robin_schedule();
    //unnecessary non blocking impl for listen
    (Lazy::force(&LISTEN))(socket, backlog)
}

static ACCEPT: Lazy<
    extern "C" fn(libc::c_int, *mut libc::sockaddr, *mut libc::socklen_t) -> libc::c_int,
> = init_hook!("accept");

#[no_mangle]
pub extern "C" fn accept(
    socket: libc::c_int,
    address: *mut libc::sockaddr,
    address_len: *mut libc::socklen_t,
) -> libc::c_int {
    impl_read_hook!((Lazy::force(&ACCEPT))(socket, address, address_len), None)
}

static SHUTDOWN: Lazy<extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int> =
    init_hook!("shutdown");

#[no_mangle]
pub extern "C" fn shutdown(socket: libc::c_int, how: libc::c_int) -> libc::c_int {
    let _ = base_coroutine::EventLoop::round_robin_schedule();
    //取消对fd的监听
    match how {
        libc::SHUT_RD => base_coroutine::EventLoop::round_robin_del_read_event(socket),
        libc::SHUT_WR => base_coroutine::EventLoop::round_robin_del_write_event(socket),
        libc::SHUT_RDWR => base_coroutine::EventLoop::round_robin_del_event(socket),
        _ => {
            crate::unix::common::set_errno(libc::EINVAL);
            return -1;
        }
    }
    (Lazy::force(&SHUTDOWN))(socket, how)
}

static POLL: Lazy<extern "C" fn(*mut libc::pollfd, libc::nfds_t, libc::c_int) -> libc::c_int> =
    init_hook!("poll");

#[no_mangle]
pub extern "C" fn poll(
    fds: *mut libc::pollfd,
    nfds: libc::nfds_t,
    timeout: libc::c_int,
) -> libc::c_int {
    let mut t = if timeout < 0 {
        libc::c_int::MAX
    } else {
        timeout
    };
    let mut x = 1;
    let mut r;
    // just check select every x ms
    loop {
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigaddset(&mut set, libc::SIGURG);
            let oldset: libc::sigset_t = std::mem::zeroed();
            r = (Lazy::force(&POLL))(fds, nfds, 0);
            libc::pthread_sigmask(libc::SIG_SETMASK, &oldset, std::ptr::null_mut());
        }
        if r != 0 || t == 0 {
            break;
        }
        usleep((t.min(x) * 1000) as libc::c_uint);
        if t != libc::c_int::MAX {
            t = if t > x { t - x } else { 0 };
        }
        if x < 16 {
            x <<= 1;
        }
    }
    r
}

static SELECT: Lazy<
    extern "C" fn(
        libc::c_int,
        *mut libc::fd_set,
        *mut libc::fd_set,
        *mut libc::fd_set,
        *mut libc::timeval,
    ) -> libc::c_int,
> = init_hook!("select");

#[no_mangle]
pub extern "C" fn select(
    nfds: libc::c_int,
    readfds: *mut libc::fd_set,
    writefds: *mut libc::fd_set,
    errorfds: *mut libc::fd_set,
    timeout: *mut libc::timeval,
) -> libc::c_int {
    let mut t = if timeout.is_null() {
        libc::c_uint::MAX
    } else {
        unsafe { ((*timeout).tv_sec as libc::c_uint) * 1_000_000 + (*timeout).tv_usec as libc::c_uint}
    };
    let mut o = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut s: [libc::fd_set; 3] = unsafe { std::mem::zeroed() };
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
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigaddset(&mut set, libc::SIGURG);
            let oldset: libc::sigset_t = std::mem::zeroed();
            r = (Lazy::force(&SELECT))(nfds, readfds, writefds, errorfds, &mut o);
            libc::pthread_sigmask(libc::SIG_SETMASK, &oldset, std::ptr::null_mut());
        }
        if r != 0 || t == 0 {
            break;
        }
        usleep(t.min(x) * 1000);
        if t != libc::c_uint::MAX {
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
}

//write相关
static SEND: Lazy<
    extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t, libc::c_int) -> libc::ssize_t,
> = init_hook!("send");

#[no_mangle]
pub extern "C" fn send(
    socket: libc::c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_expected_write_hook!((Lazy::force(&SEND))(socket, buf, len, flags), None)
}

static WRITE: Lazy<extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t) -> libc::ssize_t> =
    init_hook!("write");

#[no_mangle]
pub extern "C" fn write(
    fd: libc::c_int,
    buf: *const libc::c_void,
    count: libc::size_t,
) -> libc::ssize_t {
    impl_expected_write_hook!((Lazy::force(&WRITE))(fd, buf, count), None)
}

static WRITEV: Lazy<extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int) -> libc::ssize_t> =
    init_hook!("writev");

#[no_mangle]
pub extern "C" fn writev(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
) -> libc::ssize_t {
    impl_write_hook!((Lazy::force(&WRITEV))(fd, iov, iovcnt), None)
}

static SENDTO: Lazy<
    extern "C" fn(
        libc::c_int,
        *const libc::c_void,
        libc::size_t,
        libc::c_int,
        *const libc::sockaddr,
        libc::socklen_t,
    ) -> libc::ssize_t,
> = init_hook!("sendto");

#[no_mangle]
pub extern "C" fn sendto(
    socket: libc::c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
    addr: *const libc::sockaddr,
    addrlen: libc::socklen_t,
) -> libc::ssize_t {
    impl_expected_write_hook!(
        (Lazy::force(&SENDTO))(socket, buf, len, flags, addr, addrlen),
        None
    )
}

static SENDMSG: Lazy<
    extern "C" fn(libc::c_int, *const libc::msghdr, libc::c_int) -> libc::ssize_t,
> = init_hook!("sendmsg");

#[no_mangle]
pub extern "C" fn sendmsg(
    fd: libc::c_int,
    msg: *const libc::msghdr,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_write_hook!((Lazy::force(&SENDMSG))(fd, msg, flags), None)
}

static PWRITE: Lazy<
    extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t, libc::off_t) -> libc::ssize_t,
> = init_hook!("pwrite");

#[no_mangle]
pub extern "C" fn pwrite(
    fd: libc::c_int,
    buf: *const libc::c_void,
    count: libc::size_t,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_expected_write_hook!((Lazy::force(&PWRITE))(fd, buf, count, offset), None)
}

static PWRITEV: Lazy<
    extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int, libc::off_t) -> libc::ssize_t,
> = init_hook!("pwritev");

#[no_mangle]
pub extern "C" fn pwritev(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_write_hook!((Lazy::force(&PWRITEV))(fd, iov, iovcnt, offset), None)
}

//read相关
static RECV: Lazy<
    extern "C" fn(libc::c_int, *mut libc::c_void, libc::size_t, libc::c_int) -> libc::ssize_t,
> = init_hook!("recv");

#[no_mangle]
pub extern "C" fn recv(
    socket: libc::c_int,
    buf: *mut libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_expected_read_hook!((Lazy::force(&RECV))(socket, buf, len, flags), None)
}

static READV: Lazy<extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int) -> libc::ssize_t> =
    init_hook!("readv");

#[no_mangle]
pub extern "C" fn readv(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
) -> libc::ssize_t {
    impl_read_hook!((Lazy::force(&READV))(fd, iov, iovcnt), None)
}

static PREAD: Lazy<
    extern "C" fn(libc::c_int, *mut libc::c_void, libc::size_t, libc::off_t) -> libc::ssize_t,
> = init_hook!("pread");

#[no_mangle]
pub extern "C" fn pread(
    fd: libc::c_int,
    buf: *mut libc::c_void,
    count: libc::size_t,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_expected_read_hook!((Lazy::force(&PREAD))(fd, buf, count, offset), None)
}

static PREADV: Lazy<
    extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int, libc::off_t) -> libc::ssize_t,
> = init_hook!("preadv");

#[no_mangle]
pub extern "C" fn preadv(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_read_hook!((Lazy::force(&PREADV))(fd, iov, iovcnt, offset), None)
}

static RECVFROM: Lazy<
    extern "C" fn(
        libc::c_int,
        *mut libc::c_void,
        libc::size_t,
        libc::c_int,
        *mut libc::sockaddr,
        *mut libc::socklen_t,
    ) -> libc::ssize_t,
> = init_hook!("recvfrom");

#[no_mangle]
pub extern "C" fn recvfrom(
    socket: libc::c_int,
    buf: *mut libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
    addr: *mut libc::sockaddr,
    addrlen: *mut libc::socklen_t,
) -> libc::ssize_t {
    impl_expected_read_hook!(
        (Lazy::force(&RECVFROM))(socket, buf, len, flags, addr, addrlen),
        None
    )
}

static RECVMSG: Lazy<extern "C" fn(libc::c_int, *mut libc::msghdr, libc::c_int) -> libc::ssize_t> =
    init_hook!("recvmsg");

#[no_mangle]
pub extern "C" fn recvmsg(
    fd: libc::c_int,
    msg: *mut libc::msghdr,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_read_hook!((Lazy::force(&RECVMSG))(fd, msg, flags), None)
}
