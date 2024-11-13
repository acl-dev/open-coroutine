use crate::syscall_mod;
use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::core::PSTR;
use windows_sys::Win32::Networking::WinSock::{
    getsockopt, SOCKET, SOL_SOCKET, SO_RCVTIMEO, SO_SNDTIMEO,
};

macro_rules! impl_facade {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
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

macro_rules! impl_iocp {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        #[cfg(all(windows, feature = "iocp"))]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        #[cfg(all(windows, feature = "iocp"))]
        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                use $crate::common::constants::{CoroutineState, SyscallState};
                use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                match $crate::net::EventLoops::$syscall($($arg, )*) {
                    Ok(arc) => {
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
                                        co.name(), syscall, new_state
                                    );
                                }
                            }
                        }
                        let (lock, cvar) = &*arc;
                        let syscall_result: $result = cvar
                            .wait_while(lock.lock().expect("lock failed"),
                                |&mut result| result.is_none()
                            )
                            .expect("lock failed")
                            .expect("no syscall result")
                            .try_into()
                            .expect("IOCP syscall result overflow");
                        // fixme 错误处理
                        // if syscall_result < 0 {
                        //     let errno: std::ffi::c_int = (-syscall_result).try_into()
                        //         .expect("IOCP errno overflow");
                        //     $crate::syscall::common::set_errno(errno);
                        //     syscall_result = -1;
                        // }
                        syscall_result
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::Other {
                            self.inner.$syscall(fn_ptr, $($arg, )*)
                        } else {
                            $crate::syscall::common::set_errno(
                                windows_sys::Win32::Networking::WinSock::WSAEWOULDBLOCK.try_into().expect("overflow")
                            );
                            windows_sys::Win32::Networking::WinSock::SOCKET_ERROR.try_into().expect("overflow")
                        }
                    }
                }
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
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut r;
                loop {
                    r = self.inner.$syscall(fn_ptr, $fd, $($arg, )*);
                    if r != -1 as _ {
                        $crate::syscall::common::reset_errno();
                        break;
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        let wait_time = std::time::Duration::from_nanos(start_time
                            .saturating_add($crate::syscall::common::recv_time_limit($fd))
                            .saturating_sub($crate::common::now()))
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_read_event(
                            $fd as _,
                            Some(wait_time),
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
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($fd_type, $buf_type, $len_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut received = 0;
                let mut r = 0;
                while received < $len {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + received as usize) as windows_sys::core::PSTR,
                        $len - received,
                        $($arg, )*
                    );
                    if r != -1 {
                        $crate::syscall::common::reset_errno();
                        received += r;
                        if received >= $len || r == 0 {
                            r = received;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        let wait_time = std::time::Duration::from_nanos(start_time
                            .saturating_add($crate::syscall::common::recv_time_limit($fd))
                            .saturating_sub($crate::common::now()))
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_read_event(
                            $fd as _,
                            Some(wait_time),
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
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($fd_type, $iov_type, $iovcnt_type, $($arg_type),*) -> $result>,
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
                let start_time = $crate::common::now();
                let mut length = 0;
                let mut received = 0usize;
                let mut r = 0;
                let mut index = 0;
                for iovec in &vec {
                    let mut offset = received.saturating_sub(length);
                    length += iovec.len as usize;
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
                            arg[0] = windows_sys::Win32::Networking::WinSock::WSABUF {
                                buf: (arg[0].buf as usize + offset) as windows_sys::core::PSTR,
                                len: arg[0].len - offset as u32,
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_uint::try_from(arg.len()).unwrap_or_else(|_| {
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
                                r = received.try_into().expect("overflow");
                                break;
                            }
                            offset = received.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait read event
                            let wait_time = std::time::Duration::from_nanos(start_time
                                .saturating_add($crate::syscall::common::recv_time_limit($fd))
                                .saturating_sub($crate::common::now()))
                                .min($crate::common::constants::SLICE);
                            if $crate::net::EventLoops::wait_read_event(
                                $fd as _,
                                Some(wait_time)
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
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($fd_type, $buf_type, $len_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::common::is_blocking($fd);
                if blocking {
                    $crate::syscall::common::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut sent = 0;
                let mut r = 0;
                while sent < $len {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + sent as usize) as windows_sys::core::PSTR,
                        $len - sent,
                        $($arg, )*
                    );
                    if r != -1 {
                        $crate::syscall::common::reset_errno();
                        sent += r;
                        if sent >= $len {
                            r = sent;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait write event
                        let wait_time = std::time::Duration::from_nanos(start_time
                            .saturating_add($crate::syscall::common::send_time_limit($fd))
                            .saturating_sub($crate::common::now()))
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_write_event(
                            $fd as _,
                            Some(wait_time),
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
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($fd_type, $iov_type, $iovcnt_type, $($arg_type),*) -> $result>,
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
                let start_time = $crate::common::now();
                let mut length = 0;
                let mut sent = 0usize;
                let mut r = 0;
                let mut index = 0;
                for iovec in &vec {
                    let mut offset = sent.saturating_sub(length);
                    length += iovec.len as usize;
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
                            arg[0] = windows_sys::Win32::Networking::WinSock::WSABUF {
                                buf: (arg[0].buf as usize + offset) as windows_sys::core::PSTR,
                                len: arg[0].len - offset as u32,
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_uint::try_from(arg.len()).unwrap_or_else(|_| {
                                panic!("{} iovcnt overflow", $crate::common::constants::Syscall::$syscall)
                            }),
                            $($arg, )*
                        );
                        if r != -1 {
                            $crate::syscall::common::reset_errno();
                            sent += r as usize;
                            if sent >= length {
                                r = sent.try_into().expect("overflow");
                                break;
                            }
                            offset = sent.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait write event
                            let wait_time = std::time::Duration::from_nanos(start_time
                                .saturating_add($crate::syscall::common::send_time_limit($fd))
                                .saturating_sub($crate::common::now()))
                                .min($crate::common::constants::SLICE);
                            if $crate::net::EventLoops::wait_write_event(
                                $fd as _,
                                Some(wait_time)
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
    ( $struct_name: ident, $trait_name: ident, $($mod_name: ident)::*, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Copy, Clone, Default)]
        struct $struct_name {}

        impl $trait_name for $struct_name {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                if let Some(f) = fn_ptr {
                    (f)($($arg),*)
                } else {
                    unsafe { $($mod_name)::*::$syscall($($arg),*) }
                }
            }
        }
    }
}

syscall_mod!(
    Sleep;
    WSARecv;
    WSASend;
    WSASocketW;
    setsockopt;
    accept;
    ioctlsocket;
    listen;
    recv;
    send;
    shutdown;
    socket;
    connect;
    CreateFileW;
    SetFilePointerEx;
    WaitOnAddress
);

static NON_BLOCKING: Lazy<DashSet<SOCKET>> = Lazy::new(Default::default);

static SEND_TIME_LIMIT: Lazy<DashMap<SOCKET, u64>> = Lazy::new(Default::default);

static RECV_TIME_LIMIT: Lazy<DashMap<SOCKET, u64>> = Lazy::new(Default::default);

pub extern "system" fn set_errno(errno: windows_sys::Win32::Foundation::WIN32_ERROR) {
    unsafe { windows_sys::Win32::Foundation::SetLastError(errno) }
}

/// # Panics
/// if set fails.
pub extern "system" fn set_non_blocking(fd: SOCKET) {
    assert!(set_non_blocking_flag(fd, true), "set_non_blocking failed !");
}

/// # Panics
/// if set fails.
pub extern "system" fn set_blocking(fd: SOCKET) {
    assert!(set_non_blocking_flag(fd, false), "set_blocking failed !");
}

extern "system" fn set_non_blocking_flag(fd: SOCKET, on: bool) -> bool {
    let non_blocking = is_non_blocking(fd);
    if non_blocking == on {
        return true;
    }
    let mut argp = on.into();
    unsafe {
        windows_sys::Win32::Networking::WinSock::ioctlsocket(
            fd,
            windows_sys::Win32::Networking::WinSock::FIONBIO,
            &mut argp,
        ) == 0
    }
}

#[must_use]
pub extern "system" fn is_blocking(fd: SOCKET) -> bool {
    !is_non_blocking(fd)
}

#[must_use]
pub extern "system" fn is_non_blocking(fd: SOCKET) -> bool {
    NON_BLOCKING.contains(&fd)
}

#[must_use]
pub extern "system" fn send_time_limit(fd: SOCKET) -> u64 {
    SEND_TIME_LIMIT.get(&fd).map_or_else(
        || {
            let mut ms = 0;
            let mut len = size_of::<PSTR>() as c_int;
            assert_eq!(0, unsafe {
                getsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_SNDTIMEO,
                    std::ptr::from_mut(&mut ms).cast(),
                    &mut len,
                )
            });
            let mut time_limit = (ms as u64).saturating_mul(1_000_000);
            if 0 == time_limit {
                // 取消超时
                time_limit = u64::MAX;
            }
            assert!(SEND_TIME_LIMIT.insert(fd, time_limit).is_none());
            time_limit
        },
        |v| *v.value(),
    )
}

#[must_use]
pub extern "system" fn recv_time_limit(fd: SOCKET) -> u64 {
    RECV_TIME_LIMIT.get(&fd).map_or_else(
        || {
            let mut ms = 0;
            let mut len = size_of::<PSTR>() as c_int;
            assert_eq!(0, unsafe {
                getsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    std::ptr::from_mut(&mut ms).cast(),
                    &mut len,
                )
            });
            let mut time_limit = (ms as u64).saturating_mul(1_000_000);
            if 0 == time_limit {
                // 取消超时
                time_limit = u64::MAX;
            }
            assert!(RECV_TIME_LIMIT.insert(fd, time_limit).is_none());
            time_limit
        },
        |v| *v.value(),
    )
}
