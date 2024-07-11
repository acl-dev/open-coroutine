use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
use libc::epoll_event;
use libc::{msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use std::ffi::{c_int, c_void};

#[derive(Debug, Default)]
pub struct IoUringLinuxSyscall<I: UnixSyscall> {
    inner: I,
}

macro_rules! unsupported {
    ( $invoker: expr, $syscall:ident, $fn_ptr:expr, $($arg: expr),* $(,)* ) => {{
        $invoker.inner.$syscall($fn_ptr, $($arg, )*)
    }};
}

macro_rules! impl_io_uring {
    ( $invoker: expr, $syscall:ident, $fn_ptr:expr, $($arg: expr),* $(,)* ) => {{
        if let Ok(result) = $crate::net::event_loop::EventLoops::$syscall($($arg, )*) {
            return result;
        }
        unsupported!($invoker, $syscall, $fn_ptr, $($arg, )*)
    }};
}

impl<I: UnixSyscall> UnixSyscall for IoUringLinuxSyscall<I> {
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
        impl_io_uring!(self, sendto, fn_ptr, socket, buf, len, flags, addr, addrlen)
    }

    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> ssize_t {
        impl_io_uring!(self, write, fn_ptr, fd, buf, count)
    }

    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        impl_io_uring!(self, pwrite, fn_ptr, fd, buf, count, offset)
    }

    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t {
        impl_io_uring!(self, sendmsg, fn_ptr, fd, msg, flags)
    }
}

impl<I: LinuxSyscall> LinuxSyscall for IoUringLinuxSyscall<I> {
    extern "C" fn epoll_ctl(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> c_int {
        impl_io_uring!(self, epoll_ctl, fn_ptr, epfd, op, fd, event)
    }
}
