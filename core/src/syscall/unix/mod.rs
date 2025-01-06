use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::ffi::c_int;

macro_rules! impl_facade {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
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

macro_rules! impl_io_uring {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
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
                        //回来的时候，系统调用已经执行完毕
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
                        $crate::syscall::set_errno(errno);
                        syscall_result = -1;
                    }
                    return syscall_result;
                }
                self.inner.$syscall(fn_ptr, $($arg, )*)
            }
        }
    }
}

macro_rules! impl_io_uring_read {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
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
                fn_ptr: Option<&extern "C" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                if let Ok(arc) = $crate::net::EventLoops::$syscall($fd, $($arg, )*) {
                    use $crate::common::constants::{CoroutineState, SyscallState};
                    use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend(
                                $crate::common::now()
                                    .saturating_add($crate::syscall::recv_time_limit($fd))
                            );
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
                        //回来的时候，系统调用已经执行完毕或者超时
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, syscall_state) = co.state() {
                            match syscall_state {
                                SyscallState::Timeout => {
                                    $crate::syscall::set_errno(libc::ETIMEDOUT);
                                    return -1;
                                },
                                SyscallState::Callback => {
                                    let new_state = SyscallState::Executing;
                                    if co.syscall((), syscall, new_state).is_err() {
                                        $crate::error!(
                                            "{} change to syscall {} {} failed !",
                                            co.name(), syscall, new_state
                                        );
                                    }
                                },
                                _ => {}
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
                        $crate::syscall::set_errno(errno);
                        syscall_result = -1;
                    }
                    return syscall_result;
                }
                self.inner.$syscall(fn_ptr, $fd, $($arg, )*)
            }
        }
    }
}

macro_rules! impl_io_uring_write {
    (
        $struct_name:ident, $trait_name: ident,
        $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
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
                fn_ptr: Option<&extern "C" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                if let Ok(arc) = $crate::net::EventLoops::$syscall($fd, $($arg, )*) {
                    use $crate::common::constants::{CoroutineState, SyscallState};
                    use $crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};

                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, SyscallState::Executing) = co.state()
                        {
                            let new_state = SyscallState::Suspend(
                                $crate::common::now()
                                    .saturating_add($crate::syscall::send_time_limit($fd))
                            );
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
                        //回来的时候，系统调用已经执行完毕或者超时
                    }
                    if let Some(co) = SchedulableCoroutine::current() {
                        if let CoroutineState::Syscall((), syscall, syscall_state) = co.state() {
                            match syscall_state {
                                SyscallState::Timeout => {
                                    $crate::syscall::set_errno(libc::ETIMEDOUT);
                                    return -1;
                                },
                                SyscallState::Callback => {
                                    let new_state = SyscallState::Executing;
                                    if co.syscall((), syscall, new_state).is_err() {
                                        $crate::error!(
                                            "{} change to syscall {} {} failed !",
                                            co.name(), syscall, new_state
                                        );
                                    }
                                },
                                _ => {}
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
                        $crate::syscall::set_errno(errno);
                        syscall_result = -1;
                    }
                    return syscall_result;
                }
                self.inner.$syscall(fn_ptr, $fd,  $($arg, )*)
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
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($fd_type, $($arg_type),*) -> $result>,
                $fd: $fd_type,
                $($arg: $arg_type),*
            ) -> $result {
                let blocking = $crate::syscall::is_blocking($fd);
                if blocking {
                    $crate::syscall::set_non_blocking($fd);
                }
                let start_time = $crate::common::now();
                let mut left_time = $crate::syscall::recv_time_limit($fd);
                let mut r = -1;
                while left_time > 0 {
                    r = self.inner.$syscall(fn_ptr, $fd, $($arg, )*);
                    if r != -1 {
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
                            $fd,
                            Some(wait_time)
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
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "C" fn(
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
                        ($buf as usize + received) as *mut std::ffi::c_void,
                        $len - received,
                        $($($arg, )*)?
                    );
                    if r != -1 {
                        $crate::syscall::reset_errno();
                        received += libc::size_t::try_from(r).expect("r overflow");
                        if received >= $len || r == 0 {
                            r = received.try_into().expect("received overflow");
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
                            $fd,
                            Some(wait_time)
                        ).is_err() {
                            r = received.try_into().expect("received overflow");
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
            $($arg: ident : $arg_type: ty),*
        ) -> $result: ty
    ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "C" fn(
                        $fd_type,
                        $iov_type,
                        $iovcnt_type,
                        $($arg_type),*
                    ) -> $result
                >,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
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
                    length += iovec.iov_len;
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
                                panic!("{} iovcnt overflow", $crate::common::constants::SyscallName::$syscall)
                            }),
                            $($arg, )*
                        );
                        if r == 0 {
                            r = received.try_into().expect("received overflow");
                            std::mem::forget(vec);
                            if blocking {
                                $crate::syscall::set_blocking($fd);
                            }
                            return r;
                        } else if r != -1 {
                            $crate::syscall::reset_errno();
                            received += libc::size_t::try_from(r).expect("r overflow");
                            if received >= length {
                                r = received.try_into().expect("received overflow");
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
                                $fd,
                                Some(wait_time)
                            ).is_err() {
                                r = received.try_into().expect("received overflow");
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
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "C" fn(
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
                        ($buf as usize + sent) as *const std::ffi::c_void,
                        $len - sent,
                        $($($arg, )*)?
                    );
                    if r != -1 {
                        $crate::syscall::reset_errno();
                        sent += libc::size_t::try_from(r).expect("r overflow");
                        if sent >= $len {
                            r = sent.try_into().expect("sent overflow");
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
                            $fd,
                            Some(wait_time),
                        ).is_err() {
                            r = sent.try_into().expect("sent overflow");
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
            $($arg: ident : $arg_type: ty),*
        ) -> $result: ty
    ) => {
        #[repr(C)]
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<
                    &extern "C" fn(
                        $fd_type,
                        $iov_type,
                        $iovcnt_type,
                        $($arg_type),*
                    ) -> $result
                >,
                $fd: $fd_type,
                $iov: $iov_type,
                $iovcnt: $iovcnt_type,
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
                    length += iovec.iov_len;
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
                                panic!("{} iovcnt overflow", $crate::common::constants::SyscallName::$syscall)
                            }),
                            $($arg, )*
                        );
                        if r != -1 {
                            $crate::syscall::reset_errno();
                            sent += libc::size_t::try_from(r).expect("r overflow");
                            if sent >= length {
                                r = sent.try_into().expect("sent overflow");
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
                                $fd,
                                Some(wait_time)
                            ).is_err() {
                                r = sent.try_into().expect("sent overflow");
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
        $struct_name: ident, $trait_name: ident,
        $syscall: ident($($arg: ident : $arg_type: ty),*$(,)?) -> $result: ty
    ) => {
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

#[cfg(target_os = "linux")]
syscall_mod!(
    accept4;
    renameat2;
);
syscall_mod!(
    accept;
    close;
    connect;
    listen;
    nanosleep;
    poll;
    pread;
    preadv;
    pthread_cond_timedwait;
    pthread_mutex_lock;
    pthread_mutex_trylock;
    pthread_mutex_unlock;
    pwrite;
    pwritev;
    read;
    readv;
    recv;
    recvfrom;
    recvmsg;
    select;
    send;
    sendmsg;
    sendto;
    shutdown;
    sleep;
    socket;
    setsockopt;
    usleep;
    write;
    writev;
    mkdir;
    mkdirat;
    fsync;
    rmdir;
    renameat;
    lseek;
    link;
    unlink;
);

static SEND_TIME_LIMIT: Lazy<DashMap<c_int, u64>> = Lazy::new(Default::default);

static RECV_TIME_LIMIT: Lazy<DashMap<c_int, u64>> = Lazy::new(Default::default);

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

pub extern "C" fn reset_errno() {
    set_errno(0);
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

#[must_use]
pub extern "C" fn send_time_limit(fd: c_int) -> u64 {
    SEND_TIME_LIMIT.get(&fd).map_or_else(
        || unsafe {
            let mut tv: libc::timeval = std::mem::zeroed();
            let mut len = libc::socklen_t::try_from(size_of::<libc::timeval>()).expect("overflow");
            if libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                std::ptr::from_mut(&mut tv).cast(),
                &mut len,
            ) == -1
            {
                let error = std::io::Error::last_os_error();
                if Some(libc::ENOTSOCK) == error.raw_os_error() {
                    // not a socket
                    return u64::MAX;
                }
                panic!("getsockopt failed: {error}");
            }
            let mut time_limit = u64::try_from(tv.tv_sec)
                .expect("overflow")
                .saturating_mul(1_000_000_000)
                .saturating_add(
                    u64::try_from(tv.tv_usec)
                        .expect("overflow")
                        .saturating_mul(1_000),
                );
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
pub extern "C" fn recv_time_limit(fd: c_int) -> u64 {
    RECV_TIME_LIMIT.get(&fd).map_or_else(
        || unsafe {
            let mut tv: libc::timeval = std::mem::zeroed();
            let mut len = libc::socklen_t::try_from(size_of::<libc::timeval>()).expect("overflow");
            if libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                std::ptr::from_mut(&mut tv).cast(),
                &mut len,
            ) == -1
            {
                let error = std::io::Error::last_os_error();
                if Some(libc::ENOTSOCK) == error.raw_os_error() {
                    // not a socket
                    return u64::MAX;
                }
                panic!("getsockopt failed: {error}");
            }
            let mut time_limit = u64::try_from(tv.tv_sec)
                .expect("overflow")
                .saturating_mul(1_000_000_000)
                .saturating_add(
                    u64::try_from(tv.tv_usec)
                        .expect("overflow")
                        .saturating_mul(1_000),
                );
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
