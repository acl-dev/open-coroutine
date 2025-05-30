use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use windows_sys::core::PSTR;
use windows_sys::Win32::Networking::WinSock::{
    getsockopt, SOCKET, SOL_SOCKET, SO_RCVTIMEO, SO_SNDTIMEO, WSAENOTSOCK,
};

macro_rules! impl_syscall {
    (
        $facade_struct_name:ident, $iocp_struct_name: ident, $nio_struct_name: ident, $raw_struct_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
        #[must_use]
        pub extern "system" fn $syscall(
            fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
            $($arg: $arg_type),*
        ) -> $result {
            cfg_if::cfg_if! {
                if #[cfg(all(windows, feature = "iocp"))] {
                    static CHAIN: once_cell::sync::Lazy<
                        $facade_struct_name<$iocp_struct_name<$nio_struct_name<$raw_struct_name>>>
                    > = once_cell::sync::Lazy::new(Default::default);
                } else {
                    static CHAIN: once_cell::sync::Lazy<$facade_struct_name<$nio_struct_name<$raw_struct_name>>> =
                        once_cell::sync::Lazy::new(Default::default);
                }
            }
            CHAIN.$syscall(fn_ptr, $($arg, )*)
        }
    };
    (
        $facade_struct_name:ident, $nio_struct_name: ident, $raw_struct_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
        #[must_use]
        pub extern "system" fn $syscall(
            fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
            $($arg: $arg_type),*
        ) -> $result {
            static CHAIN: once_cell::sync::Lazy<$facade_struct_name<$nio_struct_name<$raw_struct_name>>> =
                once_cell::sync::Lazy::new(Default::default);
            CHAIN.$syscall(fn_ptr, $($arg, )*)
        }
    };
    (
        $facade_struct_name:ident, $struct_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
        #[must_use]
        pub extern "system" fn $syscall(
            fn_ptr: Option<&extern "system" fn($($arg_type),*) -> $result>,
            $($arg: $arg_type),*
        ) -> $result {
            static CHAIN: once_cell::sync::Lazy<$facade_struct_name<$struct_name >> =
                once_cell::sync::Lazy::new(Default::default);
            CHAIN.$syscall(fn_ptr, $($arg, )*)
        }
    };
}

macro_rules! impl_facade {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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
                let syscall = $crate::common::constants::SyscallName::$syscall;
                $crate::info!("enter syscall {}", syscall);
                if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
                    let new_state = $crate::common::constants::SyscallState::Executing;
                    if co.syscall((), syscall, new_state).is_err() {
                        $crate::error!("{} change to syscall {} {} failed !",
                            co.name(), syscall, new_state
                        );
                    }
                }
                let r = self.inner.$syscall(fn_ptr, $($arg, )*);
                if let Some(co) = $crate::scheduler::SchedulableCoroutine::current() {
                    if co.running().is_err() {
                        $crate::error!("{} change to running state failed !", co.name());
                    }
                }
                $crate::info!("exit syscall {} {:?} {}", syscall, r, std::io::Error::last_os_error());
                r
            }
        }
    }
}

#[allow(unused_macros)]
macro_rules! impl_iocp {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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

                if let Ok(arc) = $crate::net::EventLoops::$syscall($($arg, )*) {
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend(u64::MAX);
                            if co.syscall((), syscall, new_state).is_err() {
                                $crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(), syscall, new_state
                                );
                            }
                        }
                    }
                    if let Some(suspender) = SchedulableSuspender::current() {
                        suspender.suspend();
                        //回来的时候，系统调用已经执行完了
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Callback) = co.state()
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
                    let mut syscall_result = cvar
                        .wait_while(lock.lock().expect("lock failed"),
                            |&mut result| result.is_none()
                        )
                        .expect("lock failed")
                        .expect("no syscall result");
                    if syscall_result < 0 {
                        $crate::syscall::set_errno((-syscall_result).try_into().expect("errno overflow"));
                        syscall_result = -1;
                    }
                    return <$result>::try_from(syscall_result).expect("overflow");
                }
                self.inner.$syscall(fn_ptr, $($arg, )*)
            }
        }
    }
}

#[allow(unused_macros)]
macro_rules! impl_iocp_read {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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
                fn_ptr: Option<&extern "system" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                use $crate::common::constants::{CoroutineState, SyscallState};
                use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                if let Ok(arc) = $crate::net::EventLoops::$syscall($fd, $($arg, )*) {
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend($crate::syscall::recv_time_limit($fd));
                            if co.syscall((), syscall, new_state).is_err() {
                                $crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(), syscall, new_state
                                );
                            }
                        }
                    }
                    if let Some(suspender) = SchedulableSuspender::current() {
                        suspender.suspend();
                        //回来的时候，系统调用已经执行完了
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Callback) = co.state()
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
                    let mut syscall_result = cvar
                        .wait_while(lock.lock().expect("lock failed"),
                            |&mut result| result.is_none()
                        )
                        .expect("lock failed")
                        .expect("no syscall result");
                    if syscall_result < 0 {
                        $crate::syscall::set_errno((-syscall_result).try_into().expect("errno overflow"));
                        syscall_result = -1;
                    }
                    return <$result>::try_from(syscall_result).expect("overflow");
                }
                self.inner.$syscall(fn_ptr, $fd, $($arg, )*)
            }
        }
    }
}

#[allow(unused_macros)]
macro_rules! impl_iocp_write {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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
                fn_ptr: Option<&extern "system" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                use $crate::common::constants::{CoroutineState, SyscallState};
                use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                if let Ok(arc) = $crate::net::EventLoops::$syscall($fd, $($arg, )*) {
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend($crate::syscall::send_time_limit($fd));
                            if co.syscall((), syscall, new_state).is_err() {
                                $crate::error!(
                                    "{} change to syscall {} {} failed !",
                                    co.name(), syscall, new_state
                                );
                            }
                        }
                    }
                    if let Some(suspender) = SchedulableSuspender::current() {
                        suspender.suspend();
                        //回来的时候，系统调用已经执行完了
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Callback) = co.state()
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
                    let mut syscall_result = cvar
                        .wait_while(lock.lock().expect("lock failed"),
                            |&mut result| result.is_none()
                        )
                        .expect("lock failed")
                        .expect("no syscall result");
                    if syscall_result < 0 {
                        $crate::syscall::set_errno((-syscall_result).try_into().expect("errno overflow"));
                        syscall_result = -1;
                    }
                    return <$result>::try_from(syscall_result).expect("overflow");
                }
                self.inner.$syscall(fn_ptr, $fd, $($arg, )*)
            }
        }
    }
}

macro_rules! impl_nio_read {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::recv_time_limit($fd);
                let mut r = windows_sys::Win32::Networking::WinSock::INVALID_SOCKET;
                while left_time > 0 {
                    r = self.inner.$syscall(fn_ptr, $fd, $($arg, )*);
                    if r != windows_sys::Win32::Networking::WinSock::INVALID_SOCKET {
                        $crate::syscall::reset_errno();
                        break;
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        left_time = start_time
                            .saturating_add($crate::syscall::recv_time_limit($fd))
                            .saturating_sub($crate::common::now());
                        let wait_time = std::time::Duration::from_nanos(left_time)
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_read_event(
                            $fd.try_into().expect("overflow"),
                            Some(wait_time),
                        ).is_err() {
                            break;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        break;
                    }
                }
                if blocking {
                    $crate::syscall::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_read_buf {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident(
            $fd: ident : $fd_type: ty,
            $buf: ident : $buf_type: ty,
            $len: ident : $len_type: ty
            $(, $($arg: ident : $arg_type: ty),*)?
        ) -> $result: ty
    ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "system" fn(
                        $fd_type,
                        $buf_type,
                        $len_type
                        $(, $($arg_type),*)?
                    ) -> $result
                >,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type
                $(, $($arg: $arg_type),*)?
            ) -> $result {
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::recv_time_limit($fd);
                let mut received = 0;
                let mut r = -1;
                while received < $len && left_time > 0 {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + usize::try_from(received).expect("overflow")) as windows_sys::core::PSTR,
                        $len - received,
                        $($($arg, )*)?
                    );
                    if r != -1 {
                        $crate::syscall::reset_errno();
                        received += r;
                        if received >= $len || r == 0 {
                            r = received;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait read event
                        left_time = start_time
                            .saturating_add($crate::syscall::recv_time_limit($fd))
                            .saturating_sub($crate::common::now());
                        let wait_time = std::time::Duration::from_nanos(left_time)
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_read_event(
                            $fd.try_into().expect("overflow"),
                            Some(wait_time),
                        ).is_err() {
                            r = received;
                            break;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        break;
                    }
                }
                if blocking {
                    $crate::syscall::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_read_iovec {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident(
            $fd: ident : $fd_type: ty,
            $iov: ident : $iov_type: ty,
            $iovcnt: ident : $iovcnt_type: ty,
            $recvd: ident : $recvd_type: ty,
            $($arg: ident : $arg_type: ty),*
        ) -> $result: ty
    ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "system" fn(
                        $fd_type,
                        $iov_type,
                        $iovcnt_type,
                        $recvd_type,
                        $($arg_type),*
                    ) -> $result
                >,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
                $recvd: $recvd_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::recv_time_limit($fd);
                let vec = unsafe {
                    Vec::from_raw_parts(
                        $iov.cast_mut(),
                        $iovcnt.try_into().expect("overflow"),
                        $iovcnt.try_into().expect("overflow"),
                    )
                };
                let mut length = 0;
                let mut received = 0usize;
                let mut r = -1;
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
                    while received < length && left_time > 0 {
                        if 0 != offset {
                            arg[0] = windows_sys::Win32::Networking::WinSock::WSABUF {
                                buf: (arg[0].buf as usize + offset) as windows_sys::core::PSTR,
                                len: arg[0].len - u32::try_from(offset).expect("overflow"),
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_uint::try_from(arg.len()).unwrap_or_else(|_| {
                                panic!("{} iovcnt overflow", $crate::common::constants::SyscallName::$syscall)
                            }),
                            $recvd,
                            $($arg, )*
                        );
                        if r != -1 {
                            $crate::syscall::reset_errno();
                            received += usize::try_from(r).expect("overflow");
                            if received >= length {
                                r = 0;
                                unsafe{ $recvd.write(received.try_into().expect("overflow")) };
                                break;
                            }
                            offset = received.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait read event
                            left_time = start_time
                                .saturating_add($crate::syscall::recv_time_limit($fd))
                                .saturating_sub($crate::common::now());
                            let wait_time = std::time::Duration::from_nanos(left_time)
                                .min($crate::common::constants::SLICE);
                            if $crate::net::EventLoops::wait_read_event(
                                $fd.try_into().expect("overflow"),
                                Some(wait_time)
                            ).is_err() {
                                r = 0;
                                unsafe{ $recvd.write(received.try_into().expect("overflow")) };
                                std::mem::forget(vec);
                                if blocking {
                                    $crate::syscall::set_blocking($fd);
                                }
                                return r;
                            }
                        } else if error_kind != std::io::ErrorKind::Interrupted {
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::set_blocking($fd);
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
                    $crate::syscall::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_write_buf {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident(
            $fd: ident : $fd_type: ty,
            $buf: ident : $buf_type: ty,
            $len: ident : $len_type: ty
            $(, $($arg: ident : $arg_type: ty),*)?
        ) -> $result: ty
    ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "system" fn(
                        $fd_type,
                        $buf_type,
                        $len_type
                        $(, $($arg_type),*)?
                    ) -> $result
                >,
                $fd: $fd_type,
                $buf: $buf_type,
                $len: $len_type
                $(, $($arg: $arg_type),*)?
            ) -> $result {
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::send_time_limit($fd);
                let mut sent = 0;
                let mut r = -1;
                while sent < $len && left_time > 0 {
                    r = self.inner.$syscall(
                        fn_ptr,
                        $fd,
                        ($buf as usize + usize::try_from(sent).expect("overflow")) as windows_sys::core::PSTR,
                        $len - sent,
                        $($($arg, )*)?
                    );
                    if r != -1 {
                        $crate::syscall::reset_errno();
                        sent += r;
                        if sent >= $len {
                            r = sent;
                            break;
                        }
                    }
                    let error_kind = std::io::Error::last_os_error().kind();
                    if error_kind == std::io::ErrorKind::WouldBlock {
                        //wait write event
                        left_time = start_time
                            .saturating_add($crate::syscall::send_time_limit($fd))
                            .saturating_sub($crate::common::now());
                        let wait_time = std::time::Duration::from_nanos(left_time)
                            .min($crate::common::constants::SLICE);
                        if $crate::net::EventLoops::wait_write_event(
                            $fd.try_into().expect("overflow"),
                            Some(wait_time),
                        ).is_err() {
                            r = sent;
                            break;
                        }
                    } else if error_kind != std::io::ErrorKind::Interrupted {
                        break;
                    }
                }
                if blocking {
                    $crate::syscall::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_write_iovec {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident(
            $fd: ident : $fd_type: ty,
            $iov: ident : $iov_type: ty,
            $iovcnt: ident : $iovcnt_type: ty,
            $sent: ident : $sent_type: ty,
            $($arg: ident : $arg_type: ty),*
        ) -> $result: ty ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "system" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "system" fn(
                        $fd_type,
                        $iov_type,
                        $iovcnt_type,
                        $sent_type,
                        $($arg_type),*
                    ) -> $result
                >,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
                $sent: $sent_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::send_time_limit($fd);
                let vec = unsafe {
                    Vec::from_raw_parts(
                        $iov.cast_mut(),
                        $iovcnt.try_into().expect("overflow"),
                        $iovcnt.try_into().expect("overflow"),
                    )
                };
                let mut length = 0;
                let mut sent = 0usize;
                let mut r = -1;
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
                    while sent < length && left_time > 0 {
                        if 0 != offset {
                            arg[0] = windows_sys::Win32::Networking::WinSock::WSABUF {
                                buf: (arg[0].buf as usize + offset) as windows_sys::core::PSTR,
                                len: arg[0].len - u32::try_from(offset).expect("overflow"),
                            };
                        }
                        r = self.inner.$syscall(
                            fn_ptr,
                            $fd,
                            arg.as_ptr(),
                            std::ffi::c_uint::try_from(arg.len()).unwrap_or_else(|_| {
                                panic!("{} iovcnt overflow", $crate::common::constants::SyscallName::$syscall)
                            }),
                            $sent,
                            $($arg, )*
                        );
                        if r != -1 {
                            $crate::syscall::reset_errno();
                            sent += usize::try_from(r).expect("overflow");
                            if sent >= length {
                                r = 0;
                                unsafe{ $sent.write(sent.try_into().expect("overflow")) };
                                break;
                            }
                            offset = sent.saturating_sub(length);
                        }
                        let error_kind = std::io::Error::last_os_error().kind();
                        if error_kind == std::io::ErrorKind::WouldBlock {
                            //wait write event
                            left_time = start_time
                                .saturating_add($crate::syscall::send_time_limit($fd))
                                .saturating_sub($crate::common::now());
                            let wait_time = std::time::Duration::from_nanos(left_time)
                                .min($crate::common::constants::SLICE);
                            if $crate::net::EventLoops::wait_write_event(
                                $fd.try_into().expect("overflow"),
                                Some(wait_time)
                            ).is_err() {
                                r = 0;
                                unsafe{ $sent.write(sent.try_into().expect("overflow")) };
                                std::mem::forget(vec);
                                if blocking {
                                    $crate::syscall::set_blocking($fd);
                                }
                                return r;
                            }
                        } else if error_kind != std::io::ErrorKind::Interrupted {
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::set_blocking($fd);
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
                    $crate::syscall::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_raw {
    (
        $struct_name: ident, $trait_name: ident, $($mod_name: ident)::*,
        $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty
    ) => {
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
    WSAAccept;
    WSARecv;
    WSASend;
    WSASocketW;
    WSAPoll;
    setsockopt;
    accept;
    ioctlsocket;
    listen;
    recv;
    send;
    shutdown;
    socket;
    connect;
    select;
    CreateFileW;
    SetFilePointerEx;
    WaitOnAddress
);

static NON_BLOCKING: Lazy<DashSet<SOCKET>> = Lazy::new(Default::default);

static SEND_TIME_LIMIT: Lazy<DashMap<SOCKET, u64>> = Lazy::new(Default::default);

static RECV_TIME_LIMIT: Lazy<DashMap<SOCKET, u64>> = Lazy::new(Default::default);

pub extern "system" fn reset_errno() {
    set_errno(0);
}

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
            &raw mut argp,
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
            let mut len = c_int::try_from(size_of::<PSTR>()).expect("overflow");
            if unsafe {
                getsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_SNDTIMEO,
                    std::ptr::from_mut(&mut ms).cast(),
                    &raw mut len,
                )
            } == -1
            {
                let error = std::io::Error::last_os_error();
                if Some(WSAENOTSOCK) == error.raw_os_error() {
                    // not a socket
                    return u64::MAX;
                }
                panic!("getsockopt failed: {error}");
            }
            let mut time_limit = u64::try_from(ms)
                .expect("overflow")
                .saturating_mul(1_000_000);
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
            let mut len = c_int::try_from(size_of::<PSTR>()).expect("overflow");
            if unsafe {
                getsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    std::ptr::from_mut(&mut ms).cast(),
                    &raw mut len,
                )
            } == -1
            {
                let error = std::io::Error::last_os_error();
                if Some(WSAENOTSOCK) == error.raw_os_error() {
                    // not a socket
                    return u64::MAX;
                }
                panic!("getsockopt failed: {error}");
            }
            let mut time_limit = u64::try_from(ms)
                .expect("overflow")
                .saturating_mul(1_000_000);
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
