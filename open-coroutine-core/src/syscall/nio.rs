use crate::net::event_loop::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_errno, set_non_blocking};
#[cfg(target_os = "linux")]
use crate::syscall::LinuxSyscall;
use crate::syscall::UnixSyscall;
#[cfg(target_os = "linux")]
use libc::epoll_event;
use libc::{
    fd_set, iovec, msghdr, nfds_t, off_t, pollfd, size_t, sockaddr, socklen_t, ssize_t, timeval,
};
use std::ffi::{c_int, c_uint, c_void};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct NioLinuxSyscall<I: UnixSyscall> {
    inner: I,
}

macro_rules! impl_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut r;
        loop {
            r = $invoker.$syscall($fn_ptr, $socket, $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                break;
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                _ = $crate::net::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                );
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

macro_rules! impl_expected_batch_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov.cast_mut(), $length as usize, $length as usize)
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
            r = $invoker.$syscall($fn_ptr, $socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                received += r as usize;
                if received >= length || r == 0 {
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

macro_rules! impl_expected_batch_write_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov.cast_mut(), $length as usize, $length as usize)
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
            r = $invoker.$syscall($fn_ptr, $socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                sent += r as usize;
                if sent >= length {
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
    extern "C" fn poll(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pollfd, nfds_t, c_int) -> c_int>,
        fds: *mut pollfd,
        nfds: nfds_t,
        timeout: c_int,
    ) -> c_int {
        let mut t = if timeout < 0 { c_int::MAX } else { timeout };
        let mut x = 1;
        let mut r;
        // just check select every x ms
        loop {
            r = self.inner.poll(fn_ptr, fds, nfds, 0);
            if r != 0 || t == 0 {
                break;
            }
            _ = EventLoops::wait_just(Some(Duration::from_millis(t.min(x) as u64)));
            if t != c_int::MAX {
                t = if t > x { t - x } else { 0 };
            }
            if x < 16 {
                x <<= 1;
            }
        }
        r
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
            r = self
                .inner
                .select(fn_ptr, nfds, readfds, writefds, errorfds, &mut o);
            if r != 0 || t == 0 {
                break;
            }
            _ = EventLoops::wait_just(Some(Duration::from_millis(u64::from(t.min(x)))));
            if t != c_uint::MAX {
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
    }

    extern "C" fn socket(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> c_int {
        self.inner.socket(fn_ptr, domain, ty, protocol)
    }

    extern "C" fn listen(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        backlog: c_int,
    ) -> c_int {
        self.inner.listen(fn_ptr, socket, backlog)
    }

    extern "C" fn accept(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
        socket: c_int,
        address: *mut sockaddr,
        address_len: *mut socklen_t,
    ) -> c_int {
        impl_read_hook!(self.inner, accept, fn_ptr, socket, address, address_len)
    }

    extern "C" fn connect(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int>,
        socket: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> c_int {
        let blocking = is_blocking(socket);
        if blocking {
            set_non_blocking(socket);
        }
        let mut r;
        loop {
            r = self.inner.connect(fn_ptr, socket, address, len);
            if r == 0 {
                reset_errno();
                break;
            }
            let errno = std::io::Error::last_os_error().raw_os_error();
            if errno == Some(libc::EINPROGRESS) {
                //阻塞，直到写事件发生
                if EventLoops::wait_write_event(socket, Some(Duration::from_millis(10))).is_err() {
                    r = -1;
                    break;
                }
                let mut err: c_int = 0;
                unsafe {
                    let mut len: socklen_t = std::mem::zeroed();
                    r = libc::getsockopt(
                        socket,
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        (std::ptr::addr_of_mut!(err)).cast::<c_void>(),
                        &mut len,
                    );
                }
                if r != 0 {
                    r = -1;
                    break;
                }
                if err != 0 {
                    set_errno(err);
                    r = -1;
                    break;
                };
                unsafe {
                    let mut address = std::mem::zeroed();
                    let mut address_len = std::mem::zeroed();
                    r = libc::getpeername(socket, &mut address, &mut address_len);
                }
                if r == 0 {
                    reset_errno();
                    r = 0;
                    break;
                }
            } else if errno != Some(libc::EINTR) {
                r = -1;
                break;
            }
        }
        if blocking {
            set_blocking(socket);
        }
        if r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ETIMEDOUT) {
            set_errno(libc::EINPROGRESS);
        }
        r
    }

    extern "C" fn shutdown(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        socket: c_int,
        how: c_int,
    ) -> c_int {
        //取消对fd的监听
        match how {
            libc::SHUT_RD => EventLoops::del_read_event(socket),
            libc::SHUT_WR => EventLoops::del_write_event(socket),
            libc::SHUT_RDWR => EventLoops::del_event(socket),
            _ => {
                set_errno(libc::EINVAL);
                return -1;
            }
        };
        self.inner.shutdown(fn_ptr, socket, how)
    }

    extern "C" fn close(&self, fn_ptr: Option<&extern "C" fn(c_int) -> c_int>, fd: c_int) -> c_int {
        EventLoops::del_event(fd);
        self.inner.close(fn_ptr, fd)
    }

    extern "C" fn recv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t {
        impl_expected_read_hook!(self.inner, recv, fn_ptr, socket, buf, len, flags)
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
        impl_expected_read_hook!(
            self.inner, recvfrom, fn_ptr, socket, buf, len, flags, addr, addrlen
        )
    }

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

    extern "C" fn readv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        impl_expected_batch_read_hook!(self.inner, readv, fn_ptr, fd, iov, iovcnt,)
    }

    extern "C" fn preadv(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        impl_expected_batch_read_hook!(self.inner, preadv, fn_ptr, fd, iov, iovcnt, offset)
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

    extern "C" fn send(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t, c_int) -> ssize_t>,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> ssize_t {
        impl_expected_write_hook!(self.inner, send, fn_ptr, socket, buf, len, flags)
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

    extern "C" fn writev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> ssize_t {
        impl_expected_batch_write_hook!(self.inner, writev, fn_ptr, fd, iov, iovcnt,)
    }

    extern "C" fn pwritev(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const iovec, c_int, off_t) -> ssize_t>,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> ssize_t {
        impl_expected_batch_write_hook!(self.inner, pwritev, fn_ptr, fd, iov, iovcnt, offset)
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

    extern "C" fn accept4(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int>,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> c_int {
        impl_read_hook!(self.inner, accept4, fn_ptr, fd, addr, len, flg)
    }
}
