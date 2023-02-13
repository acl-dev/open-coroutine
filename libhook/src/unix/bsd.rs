use once_cell::sync::Lazy;
use open_coroutine_core::EventLoop;
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

#[no_mangle]
pub extern "C" fn kevent(
    kq: libc::c_int,
    changelist: *const libc::kevent,
    nchanges: libc::c_int,
    eventlist: *mut libc::kevent,
    nevents: libc::c_int,
    timeout: *const libc::timespec,
) -> libc::c_int {
    let timeout = if timeout.is_null() {
        let _ = EventLoop::round_robin_schedule();
        None
    } else {
        let ns = unsafe {
            ((*timeout).tv_sec as u64)
                .checked_mul(1_000_000_000)
                .unwrap_or(u64::MAX)
                .checked_add((*timeout).tv_nsec as u64)
                .unwrap_or(u64::MAX)
        };
        let timeout_time = timer_utils::add_timeout_time(ns);
        let _ = EventLoop::round_robin_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        match timeout_time.checked_sub(timer_utils::now()) {
            Some(v) => {
                let to = Duration::from_nanos(v);
                Some(libc::timespec {
                    tv_sec: to.as_secs().min(libc::time_t::MAX as u64) as libc::time_t,
                    tv_nsec: libc::c_long::from(to.subsec_nanos() as i32),
                })
            }
            None => return 0,
        }
    };
    let timeout = timeout
        .as_ref()
        .map(|s| s as *const _)
        .unwrap_or(std::ptr::null_mut());
    (Lazy::force(&KEVENT))(kq, changelist, nchanges, eventlist, nevents, timeout)
}
