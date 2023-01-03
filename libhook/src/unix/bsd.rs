use crate::event_loop::EventLoop;
use once_cell::sync::Lazy;
use std::time::Duration;

static KEVENT: Lazy<
    extern "C" fn(
        libc::c_int,
        *const libc::kevent,
        libc::c_int,
        *mut libc::kevent,
        libc::c_int,
        *const libc::timespec,
    ) -> libc::c_int,
> = Lazy::new(|| unsafe {
    let ptr = libc::dlsym(libc::RTLD_NEXT, b"kevent\0".as_ptr() as _);
    if ptr.is_null() {
        panic!("system kevent not found !");
    }
    std::mem::transmute(ptr)
});

pub extern "C" fn kevent(
    kq: libc::c_int,
    changelist: *const libc::kevent,
    nchanges: libc::c_int,
    eventlist: *mut libc::kevent,
    nevents: libc::c_int,
    timeout: *const libc::timespec,
) -> libc::c_int {
    let timeout = if timeout.is_null() {
        let _ = EventLoop::next_scheduler().try_schedule();
        None
    } else {
        unsafe {
            let ns = ((*timeout).tv_sec as u64)
                .checked_mul(1_000_000_000)
                .unwrap_or(u64::MAX)
                .checked_add((*timeout).tv_nsec as u64)
                .unwrap_or(u64::MAX);
            let timeout_time = timer_utils::add_timeout_time(ns);
            let _ = EventLoop::next_scheduler().try_timeout_schedule(timeout_time);
            // 可能schedule完还剩一些时间，此时本地队列没有任务可做
            match timeout_time.checked_sub(timer_utils::now()) {
                Some(v) => Some(Duration::from_nanos(v)),
                None => return 0,
            }
        }
    };
    //等待读事件
    let event_loop = EventLoop::next();
    if event_loop.add_read_event(kq).is_err() || event_loop.wait(timeout).is_err() {
        return 0;
    }
    (Lazy::force(&KEVENT))(
        kq,
        changelist,
        nchanges,
        eventlist,
        nevents,
        std::ptr::null(),
    )
}
