// check https://www.rustwiki.org.cn/en/reference/introduction.html for help information
macro_rules! init_hook {
    ( $symbol:literal ) => {
        once_cell::sync::Lazy::new(|| unsafe {
            let syscall = $symbol;
            let symbol = std::ffi::CString::new(String::from(syscall))
                .unwrap_or_else(|_| panic!("can not transfer \"{syscall}\" to CString"));
            let ptr = libc::dlsym(libc::RTLD_NEXT, symbol.as_ptr());
            assert!(!ptr.is_null(), "system call \"{syscall}\" not found !");
            std::mem::transmute(ptr)
        })
    };
}

macro_rules! impl_read_hook {
    ( ($fn: expr) ( $socket:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut r;
        loop {
            r = $fn($socket, $($arg, )*);
            if r != -1 {
                $crate::unix::reset_errno();
                break;
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                _ = open_coroutine_core::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                );
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

macro_rules! impl_expected_read_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut received = 0;
        let mut r = 0;
        while received < $length {
            r = $fn(
                $socket,
                ($buffer as usize + received) as *mut c_void,
                $length - received,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::reset_errno();
                received += r as size_t;
                if received >= $length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if open_coroutine_core::event_loop::EventLoops::wait_read_event(
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
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

macro_rules! impl_expected_batch_read_hook {
    ( ($fn: expr) ( $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov as *mut iovec, $length as usize, $length as usize)
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
            r = $fn($socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::unix::reset_errno();
                received += r as usize;
                if received >= length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if open_coroutine_core::event_loop::EventLoops::wait_read_event(
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
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

macro_rules! impl_expected_write_hook {
    ( ($fn: expr) ( $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < $length {
            r = $fn(
                $socket,
                ($buffer as usize + sent) as *const c_void,
                $length - sent,
                $($arg, )*
            );
            if r != -1 {
                $crate::unix::reset_errno();
                sent += r as size_t;
                if sent >= $length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if open_coroutine_core::event_loop::EventLoops::wait_write_event(
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
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

macro_rules! impl_expected_batch_write_hook {
    ( ($fn: expr) ( $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* )) => {{
        let socket = $socket;
        let blocking = $crate::unix::is_blocking(socket);
        if blocking {
            $crate::unix::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov as *mut iovec, $length as usize, $length as usize)
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
            r = $fn($socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::unix::reset_errno();
                sent += r as usize;
                if sent >= length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if open_coroutine_core::event_loop::EventLoops::wait_write_event(
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
            $crate::unix::set_blocking(socket);
        }
        r
    }};
}

pub mod common;

pub mod sleep;

pub mod socket;

pub mod read;

pub mod write;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
mod linux_like;

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
    set_errno(0);
}

pub extern "C" fn set_errno(errno: libc::c_int) {
    unsafe { errno_location().write(errno) }
}

extern "C" fn set_non_blocking(socket: libc::c_int) {
    assert!(
        set_non_blocking_flag(socket, true),
        "set_non_blocking failed !"
    );
}

extern "C" fn set_blocking(socket: libc::c_int) {
    assert!(
        set_non_blocking_flag(socket, false),
        "set_blocking failed !"
    );
}

extern "C" fn set_non_blocking_flag(socket: libc::c_int, on: bool) -> bool {
    let flags = unsafe { libc::fcntl(socket, libc::F_GETFL) };
    if flags < 0 {
        return false;
    }
    unsafe {
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

#[must_use]
pub extern "C" fn is_blocking(socket: libc::c_int) -> bool {
    !is_non_blocking(socket)
}

#[must_use]
pub extern "C" fn is_non_blocking(socket: libc::c_int) -> bool {
    let flags = unsafe { libc::fcntl(socket, libc::F_GETFL) };
    if flags < 0 {
        return false;
    }
    (flags & libc::O_NONBLOCK) != 0
}
