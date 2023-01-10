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
    fn errno_location() -> *mut libc::c_int;
}

pub extern "C" fn reset_errno() {
    set_errno(0)
}

pub extern "C" fn set_errno(errno: libc::c_int) {
    unsafe { errno_location().write(errno) }
}

pub extern "C" fn set_non_blocking(socket: libc::c_int, on: bool) -> bool {
    unsafe {
        let flags = libc::fcntl(socket, libc::F_GETFL);
        if flags < 0 {
            return false;
        }
        libc::fcntl(
            socket,
            libc::F_SETFL,
            if on {
                flags | libc::O_NONBLOCK
            } else {
                flags & !libc::O_NONBLOCK
            },
        ) == 0
    }
}

pub extern "C" fn is_blocking(socket: libc::c_int) -> bool {
    !is_non_blocking(socket)
}

pub extern "C" fn is_non_blocking(socket: libc::c_int) -> bool {
    unsafe {
        let flags = libc::fcntl(socket, libc::F_GETFL);
        if flags < 0 {
            return false;
        }
        (flags & libc::O_NONBLOCK) != 0
    }
}

/// check https://www.rustwiki.org.cn/en/reference/introduction.html for help information
#[macro_export]
macro_rules! impl_simple_hook {
    ( ($fn: expr) ( $socket:expr, $($arg: expr),* $(,)* ), $timeout:expr) => {{
        let ns_time = ($timeout as Option<std::time::Duration>).map(|d|d.as_nanos() as u64).unwrap_or(u64::MAX);
        let timeout_time = timer_utils::add_timeout_time(ns_time);
        let _ = base_coroutine::EventLoop::round_robin_timeout_schedule(timeout_time);
        $fn($socket ,$($arg, )*)
    }};
}

#[macro_export]
macro_rules! impl_read_hook {
    ( ($fn: expr) ( $socket:expr, $($arg: expr),* $(,)* ), $timeout:expr) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut r;
        loop {
            r = $fn($socket ,$($arg, )*);
            if r != -1 {
                $crate::unix::common::reset_errno();
                break;
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待读事件
                if let Err(e) = event_loop.wait_read_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
}

#[macro_export]
macro_rules! impl_expected_read_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr ), $timeout:expr) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut received = 0;
        let mut r;
        loop {
            r = $fn(
                $socket,
                ($buffer as usize + received) as *mut libc::c_void,
                $length - received
            );
            if r != -1 {
                $crate::unix::common::reset_errno();
                received += r as libc::size_t;
                if received == $length || r == 0 {
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待读事件
                if let Err(e) = event_loop.wait_read_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ), $timeout:expr) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut received = 0;
        let mut r;
        loop {
            r = $fn(
                $socket,
                ($buffer as usize + received) as *mut libc::c_void,
                $length - received,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::common::reset_errno();
                received += r as libc::size_t;
                if received == $length || r == 0 {
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待读事件
                if let Err(e) = event_loop.wait_read_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
}

#[macro_export]
macro_rules! impl_write_hook {
    ( ($fn: expr) ( $socket:expr, $($arg: expr),* $(,)* ), $timeout:expr ) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut r;
        loop {
            r = $fn($socket, $($arg, )*);
            if r != -1 {
                $crate::unix::common::reset_errno();
                break;
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待写事件
                if let Err(e) = event_loop.wait_write_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
}

#[macro_export]
macro_rules! impl_expected_write_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr), $timeout:expr) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut sent = 0;
        let mut r;
        loop {
            r = $fn(
                $socket,
                ($buffer as usize + sent) as *const libc::c_void,
                $length - sent
            );
            if r != -1 {
                $crate::unix::common::reset_errno();
                sent += r as libc::size_t;
                if sent == $length || r == 0 {
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待写事件
                if let Err(e) = event_loop.wait_write_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ), $timeout:expr) => {{
        let socket = $socket;
        let blocking = $crate::unix::common::is_blocking(socket);
        if blocking {
            $crate::unix::common::set_non_blocking(socket, true);
        }
        let event_loop = base_coroutine::EventLoop::next();
        let mut sent = 0;
        let mut r;
        loop {
            r = $fn(
                $socket,
                ($buffer as usize + sent) as *const libc::c_void,
                $length - sent,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::common::reset_errno();
                sent += r as libc::size_t;
                if sent == $length || r == 0 {
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //等待写事件
                if let Err(e) = event_loop.wait_write_event(socket, $timeout) {
                    match e.kind() {
                        //maybe invoke by Monitor::signal(), just ignore this
                        std::io::ErrorKind::Interrupted => $crate::unix::common::reset_errno(),
                        _ => break,
                    }
                }
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::common::set_non_blocking(socket, false);
        }
        r
    }};
}
