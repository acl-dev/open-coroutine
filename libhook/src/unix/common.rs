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
    pub fn errno_location() -> *mut libc::c_int;
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

pub extern "C" fn is_non_blocking(socket: libc::c_int) -> bool {
    unsafe {
        let flags = libc::fcntl(socket, libc::F_GETFL);
        if flags < 0 {
            return false;
        }
        (flags & libc::O_NONBLOCK) != 0
    }
}
