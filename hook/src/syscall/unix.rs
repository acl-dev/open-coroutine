use libc::{
    fd_set, iovec, msghdr, off_t, pthread_cond_t, pthread_mutex_t, size_t, sockaddr, socklen_t,
    ssize_t, timespec, timeval,
};
use std::ffi::{c_int, c_uint, c_void};

// check https://www.rustwiki.org.cn/en/reference/introduction.html for help information
#[allow(unused_macros)]
macro_rules! impl_hook {
    ( $field_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[no_mangle]
        pub extern "C" fn $syscall(
            $($arg: $arg_type),*
        ) -> $result {
            static $field_name: once_cell::sync::Lazy<
                extern "C" fn($($arg_type, )*) -> $result,
            > = once_cell::sync::Lazy::new(|| unsafe {
                let syscall: &str = open_coroutine_core::common::constants::Syscall::$syscall.into();
                let symbol = std::ffi::CString::new(String::from(syscall))
                    .unwrap_or_else(|_| panic!("can not transfer \"{syscall}\" to CString"));
                let ptr = libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr());
                assert!(!ptr.is_null(), "system call \"{syscall}\" not found !");
                std::mem::transmute(ptr)
            });
            let fn_ptr = once_cell::sync::Lazy::force(&$field_name);
            if $crate::hook() {
                return open_coroutine_core::syscall::$syscall(Some(fn_ptr), $($arg, )*);
            }
            (fn_ptr)($($arg),*)
        }
    }
}

impl_hook!(SLEEP, sleep(secs: c_uint) -> c_uint);
impl_hook!(USLEEP, usleep(microseconds: c_uint) -> c_int);
impl_hook!(NANOSLEEP, nanosleep(rqtp: *const timespec, rmtp: *mut timespec) -> c_int);
// NOTE: unhook poll due to mio's poller
// impl_hook!(POLL, poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int);
impl_hook!(SELECT, select(nfds: c_int, readfds: *mut fd_set, writefds: *mut fd_set, errorfds: *mut fd_set, timeout: *mut timeval) -> c_int);
impl_hook!(SOCKET, socket(domain: c_int, type_: c_int, protocol: c_int) -> c_int);
impl_hook!(CONNECT, connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int);
impl_hook!(LISTEN, listen(fd: c_int, backlog: c_int) -> c_int);
impl_hook!(ACCEPT, accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int);
#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
impl_hook!(ACCEPT4, accept4(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t, flg: c_int) -> c_int);
impl_hook!(SHUTDOWN, shutdown(fd: c_int, how: c_int) -> c_int);
impl_hook!(RECV, recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t);
impl_hook!(RECVFROM, recvfrom(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> ssize_t);
impl_hook!(PREAD, pread(fd: c_int, buf: *mut c_void, count: size_t, offset: off_t) -> ssize_t);
impl_hook!(READV, readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_hook!(PREADV, preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_hook!(RECVMSG, recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t);
impl_hook!(SEND, send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t);
impl_hook!(SENDTO, sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int, addr: *const sockaddr, addrlen: socklen_t) -> ssize_t);
impl_hook!(PWRITE, pwrite(fd: c_int, buf: *const c_void, count: size_t, offset: off_t) -> ssize_t);
impl_hook!(WRITEV, writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_hook!(PWRITEV, pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_hook!(SENDMSG, sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t);
impl_hook!(PTHREAD_COND_TIMEDWAIT, pthread_cond_timedwait(cond: *mut pthread_cond_t, lock: *mut pthread_mutex_t, abstime: *const timespec) -> c_int);
// NOTE: unhook pthread_mutex_lock due to stack overflow
// impl_hook!(PTHREAD_MUTEX_LOCK, pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int);
impl_hook!(PTHREAD_MUTEX_TRYLOCK, pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int);
// NOTE: unhook pthread_mutex_unlock due to stack overflow
// impl_hook!(PTHREAD_MUTEX_UNLOCK, pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int);
