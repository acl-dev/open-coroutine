use crate::epoll_event;
use crate::event_loop::EventLoop;
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
    //关注读事件
    let event_loop = EventLoop::next();
    if event_loop.add_read_event(epfd).is_err() {
        return 0;
    }
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
    //关注读事件
    let event_loop = EventLoop::next();
    if event_loop.add_read_event(epfd).is_err() {
        return 0;
    }
    let timeout = timeout_schedule(timeout);
    (Lazy::force(&EPOLL_PWAIT))(epfd, events, maxevents, timeout, sigmask)
}
