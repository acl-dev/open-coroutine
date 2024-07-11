use crate::common::{Current, Named};
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
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
