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

pub mod sleep;

pub mod socket;

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
    assert!(set_non_blocking_flag(socket, true));
}

extern "C" fn set_blocking(socket: libc::c_int) {
    assert!(set_non_blocking_flag(socket, false));
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
