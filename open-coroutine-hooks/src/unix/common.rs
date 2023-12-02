use libc::{c_int, fd_set, nfds_t, pollfd, timeval};
use once_cell::sync::Lazy;

static POLL: Lazy<extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int> = init_hook!("poll");

#[no_mangle]
pub extern "C" fn poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int {
    open_coroutine_core::syscall::poll(Some(Lazy::force(&POLL)), fds, nfds, timeout)
}

static SELECT: Lazy<
    extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
> = init_hook!("select");

#[no_mangle]
pub extern "C" fn select(
    nfds: c_int,
    readfds: *mut fd_set,
    writefds: *mut fd_set,
    errorfds: *mut fd_set,
    timeout: *mut timeval,
) -> c_int {
    open_coroutine_core::syscall::select(
        Some(Lazy::force(&SELECT)),
        nfds,
        readfds,
        writefds,
        errorfds,
        timeout,
    )
}
