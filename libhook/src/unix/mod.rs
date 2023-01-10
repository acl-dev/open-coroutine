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
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"nanosleep\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system nanosleep not found !");
        }
        std::mem::transmute(ptr)
    });

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
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"connect\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system connect not found !");
    }
    std::mem::transmute(ptr)
});

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

static LISTEN: Lazy<extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int> =
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"listen\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system listen not found !");
        }
        std::mem::transmute(ptr)
    });

#[no_mangle]
pub extern "C" fn listen(socket: libc::c_int, backlog: libc::c_int) -> libc::c_int {
    let _ = base_coroutine::EventLoop::round_robin_schedule();
    //unnecessary non blocking impl for listen
    (Lazy::force(&LISTEN))(socket, backlog)
}

static ACCEPT: Lazy<
    extern "C" fn(libc::c_int, *mut libc::sockaddr, *mut libc::socklen_t) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"accept\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system accept not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn accept(
    socket: libc::c_int,
    address: *mut libc::sockaddr,
    address_len: *mut libc::socklen_t,
) -> libc::c_int {
    //todo 非阻塞实现
    impl_simple_hook!(
        socket,
        (Lazy::force(&ACCEPT))(socket, address, address_len),
        None
    )
}

static SHUTDOWN: Lazy<extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int> =
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"shutdown\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system shutdown not found !");
        }
        std::mem::transmute(ptr)
    });

#[no_mangle]
pub extern "C" fn shutdown(socket: libc::c_int, how: libc::c_int) -> libc::c_int {
    //todo 取消对fd的监听
    impl_simple_hook!(socket, (Lazy::force(&SHUTDOWN))(socket, how), None)
}

static POLL: Lazy<extern "C" fn(*mut libc::pollfd, libc::nfds_t, libc::c_int) -> libc::c_int> =
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"poll\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system poll not found !");
        }
        std::mem::transmute(ptr)
    });

#[no_mangle]
pub extern "C" fn poll(
    fds: *mut libc::pollfd,
    nfds: libc::nfds_t,
    timeout: libc::c_int,
) -> libc::c_int {
    //todo 完善实现
    impl_simple_hook!(fds, (Lazy::force(&POLL))(fds, nfds, timeout), None)
}

static SELECT: Lazy<
    extern "C" fn(
        libc::c_int,
        *mut libc::fd_set,
        *mut libc::fd_set,
        *mut libc::fd_set,
        *mut libc::timeval,
    ) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"select\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system select not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn select(
    nfds: libc::c_int,
    readfds: *mut libc::fd_set,
    writefds: *mut libc::fd_set,
    errorfds: *mut libc::fd_set,
    timeout: *mut libc::timeval,
) -> libc::c_int {
    //todo 完善实现
    impl_simple_hook!(
        nfds,
        (Lazy::force(&SELECT))(nfds, readfds, writefds, errorfds, timeout),
        None
    )
}

//write相关
static SEND: Lazy<
    extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t, libc::c_int) -> libc::ssize_t,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"send\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system send not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn send(
    socket: libc::c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_write_hook!(socket, (Lazy::force(&SEND))(socket, buf, len, flags), None)
}

static WRITE: Lazy<extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t) -> libc::ssize_t> =
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"write\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system write not found !");
        }
        std::mem::transmute(ptr)
    });

#[no_mangle]
pub extern "C" fn write(
    fd: libc::c_int,
    buf: *const libc::c_void,
    count: libc::size_t,
) -> libc::ssize_t {
    impl_write_hook!(fd, (Lazy::force(&WRITE))(fd, buf, count), None)
}

static WRITEV: Lazy<extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int) -> libc::ssize_t> =
    Lazy::new(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, b"writev\0".as_ptr() as _);
        if ptr.is_null() {
            panic!("system writev not found !");
        }
        std::mem::transmute(ptr)
    });

#[no_mangle]
pub extern "C" fn writev(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
) -> libc::ssize_t {
    impl_write_hook!(fd, (Lazy::force(&WRITEV))(fd, iov, iovcnt), None)
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
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"sendto\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system sendto not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn sendto(
    socket: libc::c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
    addr: *const libc::sockaddr,
    addrlen: libc::socklen_t,
) -> libc::ssize_t {
    impl_write_hook!(
        socket,
        (Lazy::force(&SENDTO))(socket, buf, len, flags, addr, addrlen),
        None
    )
}

static SENDMSG: Lazy<
    extern "C" fn(libc::c_int, *const libc::msghdr, libc::c_int) -> libc::ssize_t,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"sendmsg\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system sendmsg not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn sendmsg(
    fd: libc::c_int,
    msg: *const libc::msghdr,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_write_hook!(fd, (Lazy::force(&SENDMSG))(fd, msg, flags), None)
}

static PWRITE: Lazy<
    extern "C" fn(libc::c_int, *const libc::c_void, libc::size_t, libc::off_t) -> libc::ssize_t,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"pwrite\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system pwrite not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn pwrite(
    fd: libc::c_int,
    buf: *const libc::c_void,
    count: libc::size_t,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_write_hook!(fd, (Lazy::force(&PWRITE))(fd, buf, count, offset), None)
}

static PWRITEV: Lazy<
    extern "C" fn(libc::c_int, *const libc::iovec, libc::c_int, libc::off_t) -> libc::ssize_t,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"pwritev\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system pwritev not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn pwritev(
    fd: libc::c_int,
    iov: *const libc::iovec,
    iovcnt: libc::c_int,
    offset: libc::off_t,
) -> libc::ssize_t {
    impl_write_hook!(fd, (Lazy::force(&PWRITEV))(fd, iov, iovcnt, offset), None)
}

//read相关
static RECV: Lazy<
    extern "C" fn(libc::c_int, *mut libc::c_void, libc::size_t, libc::c_int) -> libc::ssize_t,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"recv\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system recv not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn recv(
    socket: libc::c_int,
    buf: *mut libc::c_void,
    len: libc::size_t,
    flags: libc::c_int,
) -> libc::ssize_t {
    impl_read_hook!(socket, (Lazy::force(&RECV))(socket, buf, len, flags), None)
}
