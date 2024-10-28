use crate::syscall_mod;
use dashmap::DashSet;
use once_cell::sync::Lazy;
use windows_sys::Win32::Networking::WinSock::SOCKET;

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
                        if $crate::net::EventLoops::wait_read_event(
                            $fd as _,
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
                        if $crate::net::EventLoops::wait_read_event(
                            $fd as _,
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
                            if $crate::net::EventLoops::wait_read_event(
                                $fd as _,
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
                        if $crate::net::EventLoops::wait_write_event(
                            $fd as _,
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
                            if $crate::net::EventLoops::wait_write_event(
                                $fd as _,
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
    accept;
    ioctlsocket;
    listen;
    recv;
    send;
    shutdown;
    socket;
    CreateFileW;
    SetFilePointerEx;
    WaitOnAddress
);

static NON_BLOCKING: Lazy<DashSet<SOCKET>> = Lazy::new(Default::default);

pub extern "C" fn set_errno(errno: windows_sys::Win32::Foundation::WIN32_ERROR) {
    unsafe { windows_sys::Win32::Foundation::SetLastError(errno) }
}

/// # Panics
/// if set fails.
pub extern "C" fn set_non_blocking(fd: SOCKET) {
    assert!(set_non_blocking_flag(fd, true), "set_non_blocking failed !");
}

/// # Panics
/// if set fails.
pub extern "C" fn set_blocking(fd: SOCKET) {
    assert!(set_non_blocking_flag(fd, false), "set_blocking failed !");
}

extern "C" fn set_non_blocking_flag(fd: SOCKET, on: bool) -> bool {
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
pub extern "C" fn is_blocking(fd: SOCKET) -> bool {
    !is_non_blocking(fd)
}

#[must_use]
pub extern "C" fn is_non_blocking(fd: SOCKET) -> bool {
    NON_BLOCKING.contains(&fd)
}
