pub use accept::accept;
#[cfg(target_os = "linux")]
pub use accept4::accept4;
pub use close::close;
pub use connect::connect;
pub use listen::listen;
pub use nanosleep::nanosleep;
pub use recv::recv;
pub use send::send;
pub use shutdown::shutdown;
pub use sleep::sleep;
pub use socket::socket;
pub use usleep::usleep;

macro_rules! impl_facade {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
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
                use $crate::constants::{Syscall, SyscallState};
                use $crate::common::{Current, Named};
                use $crate::scheduler::SchedulableCoroutine;

                let syscall = Syscall::$syscall;
                $crate::info!("hook syscall {}", syscall);
                if let Some(co) = SchedulableCoroutine::current() {
                    let new_state = SyscallState::Executing;
                    if co.syscall((), syscall, new_state).is_err() {
                        $crate::error!("{} change to syscall {} {} failed !",
                            co.get_name(), syscall, new_state);
                    }
                }
                let r = self.inner.$syscall(fn_ptr, $($arg, )*);
                if let Some(co) = SchedulableCoroutine::current() {
                    if co.running().is_err() {
                        $crate::error!("{} change to running state failed !", co.get_name());
                    }
                }
                r
            }
        }
    }
}

macro_rules! impl_io_uring {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        #[derive(Debug, Default)]
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
                $crate::net::event_loop::EventLoops::$syscall($($arg, )*)
                    .unwrap_or_else(|_| self.inner.$syscall(fn_ptr, $($arg, )*))
            }
        }
    }
}

macro_rules! impl_nio_read {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
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
                        if $crate::net::event_loop::EventLoops::wait_read_event(
                            $fd,
                            Some(std::time::Duration::from_millis(10)),
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

macro_rules! impl_nio_expected_read {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $buf: ident : $buf_type: ty, $len: ident : $len_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
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
                        ($buf as usize + received) as *mut c_void,
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
                        if $crate::net::event_loop::EventLoops::wait_read_event(
                            $fd,
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
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_nio_expected_write {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($fd: ident : $fd_type: ty,
        $buf: ident : $buf_type: ty, $len: ident : $len_type: ty, $($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
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
                        ($buf as usize + sent) as *const c_void,
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
                        if $crate::net::event_loop::EventLoops::wait_write_event(
                            $fd,
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
                    $crate::syscall::common::set_blocking($fd);
                }
                r
            }
        }
    }
}

macro_rules! impl_raw {
    ( $struct_name: ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
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
mod recv;
mod send;
mod shutdown;
mod sleep;
mod socket;
mod usleep;
