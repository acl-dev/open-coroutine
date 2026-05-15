use libc::{
    fd_set, iovec, mode_t, msghdr, off_t, pthread_cond_t, pthread_mutex_t, size_t, sockaddr,
    socklen_t, ssize_t, timespec, timeval,
};
use std::ffi::{c_char, c_int, c_uint, c_void};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicPtr, Ordering};

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
                let syscall: &str = open_coroutine_core::common::constants::SyscallName::$syscall.into();
                let symbol = std::ffi::CString::new(String::from(syscall))
                    .unwrap_or_else(|_| panic!("can not transfer \"{syscall}\" to CString"));
                let ptr = libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr());
                assert!(!ptr.is_null(), "syscall \"{syscall}\" not found !");
                std::mem::transmute(ptr)
            });
            let fn_ptr = once_cell::sync::Lazy::force(&$field_name);
            if $crate::hook()
                || open_coroutine_core::scheduler::SchedulableCoroutine::current().is_some()
                || cfg!(feature = "ci")
            {
                return open_coroutine_core::syscall::$syscall(Some(fn_ptr), $($arg, )*);
            }
            (fn_ptr)($($arg),*)
        }
    }
}

// The following are supported syscall
impl_hook!(SLEEP, sleep(secs: c_uint) -> c_uint);
impl_hook!(USLEEP, usleep(microseconds: c_uint) -> c_int);
impl_hook!(NANOSLEEP, nanosleep(rqtp: *const timespec, rmtp: *mut timespec) -> c_int);
impl_hook!(SELECT, select(nfds: c_int, readfds: *mut fd_set, writefds: *mut fd_set, errorfds: *mut fd_set, timeout: *mut timeval) -> c_int);
impl_hook!(SOCKET, socket(domain: c_int, type_: c_int, protocol: c_int) -> c_int);
impl_hook!(SETSOCKOPT, setsockopt(socket: c_int, level: c_int, name: c_int, value: *const c_void, option_len: socklen_t) -> c_int);
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
#[cfg(not(all(target_os = "linux", feature = "io_uring")))]
impl_hook!(READ, read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t);
impl_hook!(PREAD, pread(fd: c_int, buf: *mut c_void, count: size_t, offset: off_t) -> ssize_t);
impl_hook!(READV, readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_hook!(PREADV, preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_hook!(RECVMSG, recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t);
impl_hook!(SEND, send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t);
impl_hook!(SENDTO, sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int, addr: *const sockaddr, addrlen: socklen_t) -> ssize_t);
impl_hook!(WRITE, write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t);
impl_hook!(PWRITE, pwrite(fd: c_int, buf: *const c_void, count: size_t, offset: off_t) -> ssize_t);
impl_hook!(WRITEV, writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_hook!(PWRITEV, pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_hook!(SENDMSG, sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t);
impl_hook!(PTHREAD_COND_TIMEDWAIT, pthread_cond_timedwait(cond: *mut pthread_cond_t, lock: *mut pthread_mutex_t, abstime: *const timespec) -> c_int);
impl_hook!(PTHREAD_MUTEX_TRYLOCK, pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int);
impl_hook!(MKDIR, mkdir(path: *const c_char, mode: mode_t) -> c_int);
impl_hook!(RMDIR, rmdir(path: *const c_char) -> c_int);
impl_hook!(LSEEK, lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t);
impl_hook!(LINK, link(src: *const c_char, dst: *const c_char) -> c_int);
impl_hook!(UNLINK, unlink(src: *const c_char) -> c_int);
impl_hook!(FSYNC, fsync(fd: c_int) -> c_int);
impl_hook!(MKDIRAT, mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int);
impl_hook!(RENAMEAT, renameat(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char) -> c_int);
#[cfg(target_os = "linux")]
impl_hook!(RENAMEAT2, renameat2(olddirfd: c_int, oldpath: *const c_char, newdirfd: c_int, newpath: *const c_char, flags: c_uint) -> c_int);

// On macOS, once_cell::sync::Lazy initialisation calls pthread_mutex_lock internally
// (std::sync::Mutex is backed by pthread_mutex_t on macOS), which would recurse back
// into this hook.  To break that cycle we store the real function pointer in an AtomicPtr
// (never needs a mutex) and use a per-thread re-entrancy flag.
//
// On Linux and other non-macOS platforms the Lazy init uses futex, so the plain
// impl_hook! macro is safe.  Using impl_hook! on those platforms also avoids the
// cross-coroutine deadlock that the flag introduces: if one coroutine sets the flag and
// then yields (waiting for a mutex), the next coroutine to call pthread_mutex_lock would
// see the flag and call the real blocking function, potentially deadlocking the event
// loop thread.
#[cfg(target_os = "macos")]
thread_local! {
    static PTHREAD_MUTEX_IN_HOOK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(target_os = "macos")]
static PTHREAD_MUTEX_LOCK_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "macos")]
static PTHREAD_MUTEX_UNLOCK_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int {
    let mut raw = PTHREAD_MUTEX_LOCK_PTR.load(Ordering::Acquire);
    if raw.is_null() {
        // dlsym uses its own internal locking (not pthread_mutex_lock), so this is safe
        // even when called re-entrantly.
        let ptr = unsafe { libc::dlsym(libc::RTLD_NEXT, c"pthread_mutex_lock".as_ptr()) };
        assert!(!ptr.is_null(), "pthread_mutex_lock not found!");
        let ptr = ptr.cast::<()>();
        PTHREAD_MUTEX_LOCK_PTR.store(ptr, Ordering::Release);
        raw = ptr;
    }
    let fn_ptr: extern "C" fn(*mut pthread_mutex_t) -> c_int = unsafe { std::mem::transmute(raw) };

    if PTHREAD_MUTEX_IN_HOOK.with(std::cell::Cell::get) {
        return fn_ptr(lock);
    }
    PTHREAD_MUTEX_IN_HOOK.with(|b| b.set(true));

    let result = if crate::hook()
        || open_coroutine_core::scheduler::SchedulableCoroutine::current().is_some()
        || cfg!(feature = "ci")
    {
        open_coroutine_core::syscall::pthread_mutex_lock(Some(&fn_ptr), lock)
    } else {
        fn_ptr(lock)
    };

    PTHREAD_MUTEX_IN_HOOK.with(|b| b.set(false));
    result
}

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int {
    let mut raw = PTHREAD_MUTEX_UNLOCK_PTR.load(Ordering::Acquire);
    if raw.is_null() {
        let ptr = unsafe { libc::dlsym(libc::RTLD_NEXT, c"pthread_mutex_unlock".as_ptr()) };
        assert!(!ptr.is_null(), "pthread_mutex_unlock not found!");
        let ptr = ptr.cast::<()>();
        PTHREAD_MUTEX_UNLOCK_PTR.store(ptr, Ordering::Release);
        raw = ptr;
    }
    let fn_ptr: extern "C" fn(*mut pthread_mutex_t) -> c_int = unsafe { std::mem::transmute(raw) };

    if PTHREAD_MUTEX_IN_HOOK.with(std::cell::Cell::get) {
        return fn_ptr(lock);
    }
    PTHREAD_MUTEX_IN_HOOK.with(|b| b.set(true));

    let result = if crate::hook()
        || open_coroutine_core::scheduler::SchedulableCoroutine::current().is_some()
        || cfg!(feature = "ci")
    {
        open_coroutine_core::syscall::pthread_mutex_unlock(Some(&fn_ptr), lock)
    } else {
        fn_ptr(lock)
    };

    PTHREAD_MUTEX_IN_HOOK.with(|b| b.set(false));
    result
}

// On non-macOS Unix, impl_hook! is safe for pthread_mutex_lock/unlock.
#[cfg(not(target_os = "macos"))]
impl_hook!(PTHREAD_MUTEX_LOCK, pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int);
#[cfg(not(target_os = "macos"))]
impl_hook!(PTHREAD_MUTEX_UNLOCK, pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int);

// NOTE: unhook poll due to mio's poller
// impl_hook!(POLL, poll(fds: *mut pollfd, nfds: nfds_t, timeout: c_int) -> c_int);
