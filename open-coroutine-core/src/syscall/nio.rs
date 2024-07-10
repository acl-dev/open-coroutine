use crate::net::event_loop::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_non_blocking};
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
use std::ffi::{c_int, c_void};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct NioLinuxSyscall<I: UnixSyscall> {
    inner: I,
}

macro_rules! impl_expected_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut received = 0;
        let mut r = 0;
        while received < $length {
            r = $invoker.$syscall(
                $fn_ptr,
                $socket,
                ($buffer as usize + received) as *mut c_void,
                $length - received,
                $($arg, )*
            );
            if r != -1 {
                $crate::syscall::common::reset_errno();
                received += r as size_t;
                if received >= $length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if $crate::net::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::syscall::common::set_blocking(socket);
        }
        r
    }};
}

macro_rules! impl_expected_write_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < $length {
            r = $invoker.$syscall(
                $fn_ptr,
                $socket,
                ($buffer as usize + sent) as *const c_void,
                $length - sent,
                $($arg, )*
            );
            if r != -1 {
                $crate::syscall::common::reset_errno();
                sent += r as size_t;
                if sent >= $length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if $crate::net::event_loop::EventLoops::wait_write_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::syscall::common::set_blocking(socket);
        }
        r
    }};
}

impl<I: UnixSyscall> UnixSyscall for NioLinuxSyscall<I> {
    extern "C" fn read(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
    ) -> ssize_t {
        impl_expected_read_hook!(self.inner, read, fn_ptr, fd, buf, count,)
    }

    extern "C" fn pread(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        impl_expected_read_hook!(self.inner, pread, fn_ptr, fd, buf, count, offset)
    }

    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let msghdr = unsafe { *msg };
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts(
                msghdr.msg_iov,
                msghdr.msg_iovlen as usize,
                msghdr.msg_iovlen as usize,
            )
        });
        let mut length = 0;
        let mut pices = std::collections::VecDeque::new();
        for iovec in &vec {
            length += iovec.iov_len;
            pices.push_back(length);
        }
        let mut received = 0;
        let mut r = 0;
        while received < length {
            // find from-index
            let mut from_index = 0;
            for (i, v) in pices.iter().enumerate() {
                if received < *v {
                    from_index = i;
                    break;
                }
            }
            // calculate offset
            let current_received_offset = if from_index > 0 {
                received.saturating_sub(pices[from_index.saturating_sub(1)])
            } else {
                received
            };
            // remove already received
            for _ in 0..from_index {
                _ = vec.pop_front();
                _ = pices.pop_front();
            }
            // build syscall args
            vec[0] = iovec {
                iov_base: (vec[0].iov_base as usize + current_received_offset) as *mut c_void,
                iov_len: vec[0].iov_len - current_received_offset,
            };
            cfg_if::cfg_if! {
                if #[cfg(any(
                    target_os = "linux",
                    target_os = "l4re",
                    target_os = "android",
                    target_os = "emscripten"
                ))] {
                    let len = vec.len();
                } else {
                    let len = c_int::try_from(vec.len()).unwrap();
                }
            }
            let mut new_msg = msghdr {
                msg_name: msghdr.msg_name,
                msg_namelen: msghdr.msg_namelen,
                msg_iov: vec.get_mut(0).unwrap(),
                msg_iovlen: len,
                msg_control: msghdr.msg_control,
                msg_controllen: msghdr.msg_controllen,
                msg_flags: msghdr.msg_flags,
            };
            r = self.inner.recvmsg(fn_ptr, fd, &mut new_msg, flags);
            if r != -1 {
                reset_errno();
                received += r as usize;
                if received >= length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if EventLoops::wait_read_event(fd, Some(Duration::from_millis(10))).is_err() {
                    break;
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            set_blocking(fd);
        }
        r
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
        impl_expected_write_hook!(
            self.inner, sendto, fn_ptr, socket, buf, len, flags, addr, addrlen
        )
    }

    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> ssize_t {
        impl_expected_write_hook!(self.inner, write, fn_ptr, fd, buf, count,)
    }

    extern "C" fn pwrite(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, off_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> ssize_t {
        impl_expected_write_hook!(self.inner, pwrite, fn_ptr, fd, buf, count, offset)
    }

    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let msghdr = unsafe { *msg };
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts(
                msghdr.msg_iov,
                msghdr.msg_iovlen as usize,
                msghdr.msg_iovlen as usize,
            )
        });
        let mut length = 0;
        let mut pices = std::collections::VecDeque::new();
        for iovec in &vec {
            length += iovec.iov_len;
            pices.push_back(length);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < length {
            // find from-index
            let mut from_index = 0;
            for (i, v) in pices.iter().enumerate() {
                if sent < *v {
                    from_index = i;
                    break;
                }
            }
            // calculate offset
            let current_sent_offset = if from_index > 0 {
                sent.saturating_sub(pices[from_index.saturating_sub(1)])
            } else {
                sent
            };
            // remove already sent
            for _ in 0..from_index {
                _ = vec.pop_front();
                _ = pices.pop_front();
            }
            // build syscall args
            vec[0] = iovec {
                iov_base: (vec[0].iov_base as usize + current_sent_offset) as *mut c_void,
                iov_len: vec[0].iov_len - current_sent_offset,
            };
            cfg_if::cfg_if! {
                if #[cfg(any(
                    target_os = "linux",
                    target_os = "l4re",
                    target_os = "android",
                    target_os = "emscripten"
                ))] {
                    let len = vec.len();
                } else {
                    let len = c_int::try_from(vec.len()).unwrap();
                }
            }
            let new_msg = msghdr {
                msg_name: msghdr.msg_name,
                msg_namelen: msghdr.msg_namelen,
                msg_iov: vec.get_mut(0).unwrap(),
                msg_iovlen: len,
                msg_control: msghdr.msg_control,
                msg_controllen: msghdr.msg_controllen,
                msg_flags: msghdr.msg_flags,
            };
            r = self.inner.sendmsg(fn_ptr, fd, &new_msg, flags);
            if r != -1 {
                reset_errno();
                sent += r as usize;
                if sent >= length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if EventLoops::wait_write_event(fd, Some(Duration::from_millis(10))).is_err() {
                    break;
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            set_blocking(fd);
        }
        r
    }
}

#[cfg(target_os = "linux")]
impl<I: LinuxSyscall> LinuxSyscall for NioLinuxSyscall<I> {
    extern "C" fn epoll_ctl(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int, *mut epoll_event) -> c_int>,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> c_int {
        self.inner.epoll_ctl(fn_ptr, epfd, op, fd, event)
    }
}
