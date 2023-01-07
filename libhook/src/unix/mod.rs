use crate::event_loop::EventLoop;
use crate::unix::common::*;
use once_cell::sync::Lazy;
use std::ffi::c_void;
use std::io::ErrorKind;

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
        let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
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
    let event_loop = EventLoop::next();
    let mut r;
    loop {
        r = (Lazy::force(&CONNECT))(socket, address, len);
        if r == 0 {
            reset_errno();
            break;
        }
        let error = std::io::Error::last_os_error();
        let errno = error.raw_os_error();
        if errno == Some(libc::EINPROGRESS) {
            //等待写事件
            if event_loop.wait_write_event(socket, None).is_err() {
                r = -1;
                break;
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
                    r = 0;
                    reset_errno();
                    break;
                };
                set_errno(err);
            }
            r = -1;
            break;
        } else if error.kind() != ErrorKind::Interrupted {
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
    let _ = EventLoop::round_robin_schedule();
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
    let _ = EventLoop::round_robin_schedule();
    //todo 非阻塞实现
    (Lazy::force(&ACCEPT))(socket, address, address_len)
}

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
    let blocking = is_blocking(socket);
    //阻塞，epoll_wait/kevent等待直到写事件
    if blocking {
        set_non_blocking(socket, true);
    }
    let event_loop = EventLoop::next();
    let mut r;
    loop {
        r = (Lazy::force(&SEND))(socket, buf, len, flags);
        if r != -1 {
            reset_errno();
            break;
        }
        let error_kind = std::io::Error::last_os_error().kind();
        if error_kind == ErrorKind::WouldBlock {
            //等待写事件
            if event_loop.wait_write_event(socket, None).is_err() {
                break;
            }
        } else if error_kind != ErrorKind::Interrupted {
            break;
        }
    }
    if blocking {
        set_non_blocking(socket, false);
    }
    r
}

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
    let blocking = is_blocking(socket);
    //阻塞，epoll_wait/kevent等待直到读事件
    if blocking {
        set_non_blocking(socket, true);
    }
    let event_loop = EventLoop::next();
    let mut r;
    loop {
        r = (Lazy::force(&RECV))(socket, buf, len, flags);
        if r != -1 {
            reset_errno();
            break;
        }
        let error_kind = std::io::Error::last_os_error().kind();
        if error_kind == ErrorKind::WouldBlock {
            //等待读事件
            if event_loop.wait_read_event(socket, None).is_err() {
                break;
            }
        } else if error_kind != ErrorKind::Interrupted {
            break;
        }
    }
    if blocking {
        set_non_blocking(socket, false);
    }
    r
}
