use std::ffi::c_int;

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
pub extern "C" fn set_non_blocking(socket: c_int) {
    assert!(
        set_non_blocking_flag(socket, true),
        "set_non_blocking failed !"
    );
}

/// # Panics
/// if set fails.
pub extern "C" fn set_blocking(socket: c_int) {
    assert!(
        set_non_blocking_flag(socket, false),
        "set_blocking failed !"
    );
}

extern "C" fn set_non_blocking_flag(socket: c_int, on: bool) -> bool {
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
pub extern "C" fn is_blocking(socket: c_int) -> bool {
    !is_non_blocking(socket)
}

#[must_use]
pub extern "C" fn is_non_blocking(socket: c_int) -> bool {
    let flags = unsafe { libc::fcntl(socket, libc::F_GETFL) };
    if flags < 0 {
        return false;
    }
    (flags & libc::O_NONBLOCK) != 0
}

#[macro_export]
macro_rules! log_syscall {
    ( $socket:expr, $done:expr, $once_result:expr ) => {
        #[cfg(feature = "logs")]
        if let Some(coroutine) = $crate::scheduler::SchedulableCoroutine::current() {
            $crate::info!(
                "{} {} {} {} {} {}",
                coroutine.get_name(),
                coroutine.state(),
                $socket,
                $done,
                $once_result,
                std::io::Error::last_os_error(),
            );
        }
    };
}

#[macro_export]
macro_rules! impl_non_blocking {
    ( $socket:expr, $impls:expr ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let r = $impls;
        if blocking {
            $crate::syscall::common::set_blocking(socket);
        }
        return r;
    }};
}
