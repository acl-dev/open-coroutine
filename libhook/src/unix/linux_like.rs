use base_coroutine::{epoll_event, EventLoop};
use once_cell::sync::Lazy;

fn timeout_schedule(timeout: libc::c_int) -> libc::c_int {
    if timeout < 0 {
        let _ = EventLoop::round_robin_schedule();
        -1
    } else {
        let ns = timeout.checked_mul(1_000_000).unwrap_or(libc::c_int::MAX);
        let timeout_time = timer_utils::add_timeout_time(ns as u64);
        let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        match timeout_time.checked_sub(timer_utils::now()) {
            Some(v) => (v / 1_000_000) as libc::c_int,
            None => return 0,
        }
    }
}

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
    let timeout = timeout_schedule(timeout);
    (Lazy::force(&EPOLL_WAIT))(epfd, events, maxevents, timeout)
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
    let timeout = timeout_schedule(timeout);
    (Lazy::force(&EPOLL_PWAIT))(epfd, events, maxevents, timeout, sigmask)
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
    impl_read_hook!((Lazy::force(&ACCEPT4))(fd, addr, len, flg), None)
}
