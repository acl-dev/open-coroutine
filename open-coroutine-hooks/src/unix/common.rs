use libc::{c_int, c_uint, fd_set, nfds_t, pollfd, timeval};
use once_cell::sync::Lazy;

static POLL: Lazy<extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int> = init_hook!("poll");

#[allow(clippy::many_single_char_names, clippy::cast_sign_loss)]
#[no_mangle]
pub extern "C" fn poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int {
    open_coroutine_core::unbreakable!(
        {
            let mut t = if timeout < 0 { c_int::MAX } else { timeout };
            let mut x = 1;
            let mut r;
            // just check select every x ms
            loop {
                r = Lazy::force(&POLL)(fds, nfds, 0);
                if r != 0 || t == 0 {
                    break;
                }
                _ = crate::unix::sleep::usleep((t.min(x) * 1000) as c_uint);
                if t != c_int::MAX {
                    t = if t > x { t - x } else { 0 };
                }
                if x < 16 {
                    x <<= 1;
                }
            }
            r
        },
        "poll"
    )
}

static SELECT: Lazy<
    extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
> = init_hook!("select");

#[allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#[no_mangle]
pub extern "C" fn select(
    nfds: c_int,
    readfds: *mut fd_set,
    writefds: *mut fd_set,
    errorfds: *mut fd_set,
    timeout: *mut timeval,
) -> c_int {
    open_coroutine_core::unbreakable!(
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
                r = Lazy::force(&SELECT)(nfds, readfds, writefds, errorfds, &mut o);
                if r != 0 || t == 0 {
                    break;
                }
                _ = crate::unix::sleep::usleep(t.min(x) * 1000);
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
        },
        "select"
    )
}
