use crate::epoll_event;
use crate::event_loop::EventLoop;
use crate::unix::common::{is_blocking, reset_errno, set_non_blocking};
use once_cell::sync::Lazy;
use std::io::ErrorKind;

static EPOLL_WAIT: Lazy<
    extern "C" fn(libc::c_int, *mut epoll_event, libc::c_int, libc::c_int) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"epoll_wait\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system epoll_wait not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn epoll_wait(
    epfd: libc::c_int,
    events: *mut epoll_event,
    maxevents: libc::c_int,
    timeout: libc::c_int,
) -> libc::c_int {
    let nanos_time = if timeout < 0 {
        u64::MAX
    } else {
        (timeout as u64).checked_mul(1_000_000).unwrap_or(u64::MAX)
    };
    let timeout_time = timer_utils::add_timeout_time(nanos_time);
    let mut r;
    loop {
        let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        let schedule_finished_time = timer_utils::now();
        let left_time = match timeout_time.checked_sub(schedule_finished_time) {
            Some(v) => v,
            None => return 0,
        };
        let timeout = (left_time / 1_000_000) as libc::c_int;
        r = (Lazy::force(&EPOLL_WAIT))(epfd, events, maxevents, timeout);
        if r != -1 {
            reset_errno();
            return r;
        }
    }
}

static EPOLL_PWAIT: Lazy<
    extern "C" fn(
        libc::c_int,
        *mut epoll_event,
        libc::c_int,
        libc::c_int,
        *const libc::sigset_t,
    ) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"epoll_pwait\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system epoll_pwait not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn epoll_pwait(
    epfd: libc::c_int,
    events: *mut epoll_event,
    maxevents: libc::c_int,
    timeout: libc::c_int,
    sigmask: *const libc::sigset_t,
) -> libc::c_int {
    let nanos_time = if timeout < 0 {
        u64::MAX
    } else {
        (timeout as u64).checked_mul(1_000_000).unwrap_or(u64::MAX)
    };
    let timeout_time = timer_utils::add_timeout_time(nanos_time);
    let mut r;
    loop {
        let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        let schedule_finished_time = timer_utils::now();
        let left_time = match timeout_time.checked_sub(schedule_finished_time) {
            Some(v) => v,
            None => return 0,
        };
        let timeout = (left_time / 1_000_000) as libc::c_int;
        r = (Lazy::force(&EPOLL_PWAIT))(epfd, events, maxevents, timeout, sigmask);
        if r != -1 {
            reset_errno();
            return r;
        }
    }
}

static ACCEPT4: Lazy<
    extern "C" fn(
        libc::c_int,
        *mut libc::sockaddr,
        *mut libc::socklen_t,
        libc::c_int,
    ) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"accept4\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system accept4 not found !");
    }
    std::mem::transmute(ptr)
});

#[no_mangle]
pub extern "C" fn accept4(
    fd: libc::c_int,
    addr: *mut libc::sockaddr,
    len: *mut libc::socklen_t,
    flg: libc::c_int,
) -> libc::c_int {
    let blocking = is_blocking(fd);
    //阻塞，epoll_wait/kevent等待直到读事件
    if blocking {
        set_non_blocking(fd, true);
    }
    let event_loop = EventLoop::next();
    let mut r;
    loop {
        r = (Lazy::force(&ACCEPT4))(fd, addr, len, flg);
        if r != -1 {
            reset_errno();
            break;
        }
        let error_kind = std::io::Error::last_os_error().kind();
        if error_kind == ErrorKind::WouldBlock {
            //等待读事件
            if event_loop.wait_read_event(fd, None).is_err() {
                break;
            }
        } else if error_kind != ErrorKind::Interrupted {
            break;
        }
    }
    if blocking {
        set_non_blocking(fd, false);
    }
    r
}
