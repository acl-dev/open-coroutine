use std::ffi::c_int;

pub use accept::accept;
#[cfg(target_os = "linux")]
pub use accept4::accept4;
pub use close::close;
pub use connect::connect;
pub use listen::listen;
pub use nanosleep::nanosleep;
pub use poll::poll;
pub use pread::pread;
pub use preadv::preadv;
pub use pthread_cond_timedwait::pthread_cond_timedwait;
pub use pthread_mutex_lock::pthread_mutex_lock;
pub use pthread_mutex_trylock::pthread_mutex_trylock;
pub use pthread_mutex_unlock::pthread_mutex_unlock;
pub use pwrite::pwrite;
pub use pwritev::pwritev;
pub use read::read;
pub use readv::readv;
pub use recv::recv;
pub use recvfrom::recvfrom;
pub use recvmsg::recvmsg;
pub use select::select;
pub use send::send;
pub use sendmsg::sendmsg;
pub use sendto::sendto;
pub use shutdown::shutdown;
pub use sleep::sleep;
pub use socket::socket;
pub use usleep::usleep;
pub use write::write;
pub use writev::writev;

macro_rules! impl_facade {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                let syscall = $crate::common::constants::Syscall::$syscall;
                $crate::info!("enter syscall {}", syscall);
                if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
                    let new_state = $crate::common::constants::SyscallState::Executing;
                    if co.syscall((), syscall, new_state).is_err() {
                        $crate::error!("{} change to syscall {} {} failed !",
                            co.name(), syscall, new_state);
                    }
                }
                let r = self.inner.$syscall(fn_ptr, $($arg, )*);
                if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
                    if co.running().is_err() {
                        $crate::error!("{} change to running state failed !", co.name());
                    }
                }
                $crate::info!("exit syscall {}", syscall);
                r
            }
        }
    }
}

macro_rules! impl_io_uring {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                if let Ok(arc) = $crate::net::EventLoops::$syscall($($arg, )*) {
                    use $crate::common::constants::{CoroutineState, SyscallState};
                    use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::SystemCall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend(u64::MAX);
                            if co.syscall((), syscall, new_state).is_err() {
                                $crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(),
                                    syscall,
                                    new_state
                                );
                            }
                        }
                    }
                    if let Some(suspender) = SchedulableSuspender::current() {
                        suspender.suspend();
                        //回来的时候，系统调用已经执行完了
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::SystemCall((), syscall, SyscallState::Callback) = co.state()
                        {
                            let new_state = SyscallState::Executing;
                            if co.syscall((), syscall, new_state).is_err() {
                                $crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(),
                                    syscall,
                                    new_state
                                );
                            }
                        }
                    }
                    let (lock, cvar) = &*arc;
                    let mut syscall_result: $result = cvar
                        .wait_while(lock.lock().expect("lock failed"),
                            |&mut result| result.is_none()
                        )
                        .expect("lock failed")
                        .expect("no syscall result")
                        .try_into()
                        .expect("io_uring syscall result overflow");
                    if syscall_result < 0 {
                        let errno: std::ffi::c_int = (-syscall_result).try_into()
                            .expect("io_uring errno overflow");
                        $crate::syscall::common::set_errno(errno);
                        syscall_result = -1;
                    }
                    return syscall_result;
                }
                self.inner.$syscall(fn_ptr, $($arg, )*)
            }
        }
    }
}

macro_rules! impl_nio_read {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let mut r;
                loop {
                    r = self.inner.$syscall(fn_ptr, $fd, $($arg, )*);
                    if r != -1 {
                        $crate::syscall::common::reset_errno();
                        break;
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        if $crate::net::EventLoops::wait_read_event(
                            $fd,
                            Some($crate::common::constants::SLICE),
                        ).is_err() {
                            break;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        break;
                    }
                }
                if blocking {
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_read_buf {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $buf: ident : $buf_type: ty, $len: ident : $len_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $buf_type, $len_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let mut received = 0;
                let mut r = 0;
                while received < $len {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + received) as *mut std::ffi::c_void,
                        $len - received,
                        $($arg, )*
                    );
                    if r != -1 {
                        $crate::syscall::common::reset_errno();
                        received += r as size_t;
                        if received >= $len || r == 0 {
                            r = received as ssize_t;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        if $crate::net::EventLoops::wait_read_event(
                            $fd,
                            Some($crate::common::constants::SLICE),
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
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_read_iovec {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $iov: ident : $iov_type: ty, $iovcnt: ident : $iovcnt_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $iov_type, $iovcnt_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let vec = unsafe { Vec::from_raw_parts($iov.cast_mut(), $iovcnt as usize, $iovcnt as usize) };
                let mut length = 0;
                let mut received = 0usize;
                let mut r = 0;
                let mut index = 0;
                for iovec in &vec {
                    let mut offset = received.saturating_sub(length);
                    length += iovec.iov_len;
                    if received > length {
                        index += 1;
                        continue;
                    }
                    let mut arg = Vec::new();
                    for i in vec.iter().skip(index) {
                        arg.push(*i);
                    }
                    while received < length {
                        if 0 != offset {
                            arg[0] = libc::iovec {
                                iov_base: (arg[0].iov_base as usize + offset) as *mut std::ffi::c_void,
                                iov_len: arg[0].iov_len - offset,
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_int::try_from(arg.len()).unwrap_or_else(|_| {
                                panic!("{} iovcnt overflow", $crate::common::constants::Syscall::$syscall)
                            }),
                            $($arg, )*
                        );
                        if r == 0 {
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::common::set_blocking($fd);
                            }
                            return r;
                        } else if r != -1 {
                            $crate::syscall::common::reset_errno();
                            received += r as usize;
                            if received >= length {
                                r = received as ssize_t;
                                break;
                            }
                            offset = received.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait read event
                            if $crate::net::EventLoops::wait_read_event(
                                $fd,
                                Some($crate::common::constants::SLICE)
                            ).is_err() {
                                std::mem::forget(vec);
                                if blocking {
                                    $crate::syscall::common::set_blocking($fd);
                                }
                                return r;
                            }
                        } else if error_kind != std::io::ErrorKind::Interrupted {
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::common::set_blocking($fd);
                            }
                            return r;
                        }
                    }
                    if received >= length {
                        index += 1;
                    }
                }
                std::mem::forget(vec);
                if blocking {
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_write_buf {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $buf: ident : $buf_type: ty, $len: ident : $len_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $buf_type, $len_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let mut sent = 0;
                let mut r = 0;
                while sent < $len {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + sent) as *const std::ffi::c_void,
                        $len - sent,
                        $($arg, )*
                    );
                    if r != -1 {
                        $crate::syscall::common::reset_errno();
                        sent += r as size_t;
                        if sent >= $len {
                            r = sent as ssize_t;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait write event
                        if $crate::net::EventLoops::wait_write_event(
                            $fd,
                            Some($crate::common::constants::SLICE),
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
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_write_iovec {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $iov: ident : $iov_type: ty, $iovcnt: ident : $iovcnt_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $iov_type, $iovcnt_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let vec = unsafe { Vec::from_raw_parts($iov.cast_mut(), $iovcnt as usize, $iovcnt as usize) };
                let mut length = 0;
                let mut sent = 0usize;
                let mut r = 0;
                let mut index = 0;
                for iovec in &vec {
                    let mut offset = sent.saturating_sub(length);
                    length += iovec.iov_len;
                    if sent > length {
                        index += 1;
                        continue;
                    }
                    let mut arg = Vec::new();
                    for i in vec.iter().skip(index) {
                        arg.push(*i);
                    }
                    while sent < length {
                        if 0 != offset {
                            arg[0] = libc::iovec {
                                iov_base: (arg[0].iov_base as usize + offset) as *mut std::ffi::c_void,
                                iov_len: arg[0].iov_len - offset,
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_int::try_from(arg.len()).unwrap_or_else(|_| {
                                panic!("{} iovcnt overflow", $crate::common::constants::Syscall::$syscall)
                            }),
                            $($arg, )*
                        );
                        if r != -1 {
                            $crate::syscall::common::reset_errno();
                            sent += r as usize;
                            if sent >= length {
                                r = sent as ssize_t;
                                break;
                            }
                            offset = sent.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait write event
                            if $crate::net::EventLoops::wait_write_event(
                                $fd,
                                Some($crate::common::constants::SLICE)
                            ).is_err() {
                                std::mem::forget(vec);
                                if blocking {
                                    $crate::syscall::common::set_blocking($fd);
                                }
                                return r;
                            }
                        } else if error_kind != std::io::ErrorKind::Interrupted {
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::common::set_blocking($fd);
                            }
                            return r;
                        }
                    }
                    if sent >= length {
                        index += 1;
                    }
                }
                std::mem::forget(vec);
                if blocking {
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_raw {
    ( $struct_name: ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Copy, Clone, Default)]
        struct $struct_name {}

        impl $trait_name for $struct_name {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                if let Some(f) = fn_ptr {
                    (f)($($arg),*)
                } else {
                    unsafe { libc::$syscall($($arg),*) }
                }
            }
        }
    }
}

mod accept;
#[cfg(target_os = "linux")]
mod accept4;
mod close;
mod connect;
mod listen;
mod nanosleep;
mod poll;
mod pread;
mod preadv;
mod pthread_cond_timedwait;
mod pthread_mutex_lock;
mod pthread_mutex_trylock;
mod pthread_mutex_unlock;
mod pwrite;
mod pwritev;
mod read;
mod readv;
mod recv;
mod recvfrom;
mod recvmsg;
mod select;
mod send;
mod sendmsg;
mod sendto;
mod shutdown;
mod sleep;
mod socket;
mod usleep;
mod write;
mod writev;

extern "C" {
    #[cfg(not(any(target_os = "dragonfly", target_os = "vxworks")))]
    #[cfg_attr(
        any(
            target_os = "linux",
            target_os = "emscripten",
            target_os = "fuchsia",
            target_os = "l4re"
        ),
        link_name = "__errno_location"
    )]
    #[cfg_attr(
        any(
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "android",
            target_os = "redox",
            target_env = "newlib"
        ),
        link_name = "__errno"
    )]
    #[cfg_attr(
        any(target_os = "solaris", target_os = "illumos"),
        link_name = "___errno"
    )]
    #[cfg_attr(
        any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "watchos"
        ),
        link_name = "__error"
    )]
    #[cfg_attr(target_os = "haiku", link_name = "_errnop")]
    fn errno_location() -> *mut c_int;
}

pub extern "C" fn set_errno(errno: c_int) {
    unsafe { errno_location().write(errno) }
}

/// # Panics
/// if set fails.
pub extern "C" fn set_non_blocking(fd: c_int) {
    assert!(set_non_blocking_flag(fd, true), "set_non_blocking failed !");
}

/// # Panics
/// if set fails.
pub extern "C" fn set_blocking(fd: c_int) {
    assert!(set_non_blocking_flag(fd, false), "set_blocking failed !");
}

extern "C" fn set_non_blocking_flag(fd: c_int, on: bool) -> bool {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return false;
    }
    unsafe {
        libc::fcntl(
            fd,
            libc::F_SETFL,
            if on {
                flags | libc::O_NONBLOCK
            } else {
                flags & !libc::O_NONBLOCK
            },
        ) == 0
    }
}

#[must_use]
pub extern "C" fn is_blocking(fd: c_int) -> bool {
    !is_non_blocking(fd)
}

#[must_use]
pub extern "C" fn is_non_blocking(fd: c_int) -> bool {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return false;
    }
    (flags & libc::O_NONBLOCK) != 0
}
