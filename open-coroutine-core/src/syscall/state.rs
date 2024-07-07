use crate::common::{Current, Named};
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timeval,
};
use std::ffi::{c_int, c_void};

#[derive(Debug, Default)]
pub struct StateLinuxSyscall<I: UnixSyscall> {
    inner: I,
}

macro_rules! syscall_state {
    ( $invoker: expr , $syscall: ident, $($arg: expr),* $(,)* ) => {{
        let syscall = $crate::constants::Syscall::$syscall;
        $crate::info!("{} hooked", syscall);
        if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
            if co
                .syscall((), syscall, $crate::constants::SyscallState::Executing)
                .is_err()
            {
                $crate::error!("{} change to syscall state failed !", co.get_name());
            }
        }
        let r = $invoker.inner.$syscall($($arg, )*);
        if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
            if co.running().is_err() {
                $crate::error!("{} change to running state failed !", co.get_name());
            }
        }
        return r;
    }};
}

impl<I: UnixSyscall> UnixSyscall for StateLinuxSyscall<I> {
    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int {
        syscall_state!(self, poll, fn_ptr, fds, nfds, timeout)
    }

    extern "C" fn select(
        &self,
        fn_ptr: Option<
            &extern "C" fn(c_int, *mut fd_set, *mut fd_set, *mut fd_set, *mut timeval) -> c_int,
        >,
        nfds: c_int,
        readfds: *mut fd_set,
        writefds: *mut fd_set,
        errorfds: *mut fd_set,
        timeout: *mut timeval,
    ) -> c_int {
        syscall_state!(self, select, fn_ptr, nfds, readfds, writefds, errorfds, timeout)
    }

    extern "C" fn recvfrom(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *mut c_void,
                size_t,
                c_int,
                *mut sockaddr,
                *mut socklen_t,
            ) -> ssize_t,
        >,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
    ) -> ssize_t {
        syscall_state!(self, recvfrom, fn_ptr, socket, buf, len, flags, addr, addrlen)
    }

    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
    ) -> ssize_t {
        syscall_state!(self, read, fn_ptr, fd, buf, count)
    }

    extern "C" fn pread(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        syscall_state!(self, pread, fn_ptr, fd, buf, count, offset)
    }

    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        syscall_state!(self, readv, fn_ptr, fd, iov, iovcnt)
    }

    extern "C" fn preadv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        syscall_state!(self, preadv, fn_ptr, fd, iov, iovcnt, offset)
    }

    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t {
        syscall_state!(self, recvmsg, fn_ptr, fd, msg, flags)
    }

    extern "C" fn sendto(
        &self,
        fn_ptr: Option<
            &extern "C" fn(
                c_int,
                *const c_void,
                size_t,
                c_int,
                *const sockaddr,
                socklen_t,
            ) -> ssize_t,
        >,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> ssize_t {
        syscall_state!(self, sendto, fn_ptr, socket, buf, len, flags, addr, addrlen)
    }

    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> ssize_t {
        syscall_state!(self, write, fn_ptr, fd, buf, count)
    }

    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        syscall_state!(self, pwrite, fn_ptr, fd, buf, count, offset)
    }

    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        syscall_state!(self, writev, fn_ptr, fd, iov, iovcnt)
    }

    extern "C" fn pwritev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        syscall_state!(self, pwritev, fn_ptr, fd, iov, iovcnt, offset)
    }

    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t {
        syscall_state!(self, sendmsg, fn_ptr, fd, msg, flags)
    }
}

#[cfg(target_os = "linux")]
impl<I: LinuxSyscall> LinuxSyscall for StateLinuxSyscall<I> {
    extern "C" fn epoll_ctl(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> c_int {
        syscall_state!(self, epoll_ctl, fn_ptr, epfd, op, fd, event)
    }
}
