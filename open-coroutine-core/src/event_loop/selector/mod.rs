macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

macro_rules! debug_detail {
($type: ident ($event_type: ty), $test: path,
    $($(#[$target: meta])* $libc: ident :: $flag: ident),+ $(,)*) => {
        struct $type($event_type);

        impl fmt::Debug for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut written_one = false;
                $(
                    $(#[$target])*
                    #[allow(clippy::bad_bit_mask)] // Apparently some flags are zero.
                    {
                        // Windows doesn't use `libc` but the `afd` module.
                        if $test(&self.0, &$libc :: $flag) {
                            if !written_one {
                                write!(f, "{}", stringify!($flag))?;
                                written_one = true;
                            } else {
                                write!(f, "|{}", stringify!($flag))?;
                            }
                        }
                    }
                )+
                if !written_one {
                    write!(f, "(empty)")
                } else {
                    Ok(())
                }
            }
        }
    };
}

#[cfg(any(
    target_os = "android",
    target_os = "illumos",
    target_os = "linux",
    target_os = "redox",
))]
mod epoll;

#[cfg(any(
    target_os = "android",
    target_os = "illumos",
    target_os = "linux",
    target_os = "redox",
))]
pub(crate) use self::epoll::{event, Event, Events, Selector};

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod kqueue;

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub(crate) use self::kqueue::{event, Event, Events, Selector};

/// Lowest file descriptor used in `Selector::try_clone`.
///
/// # Notes
///
/// Usually fds 0, 1 and 2 are standard in, out and error. Some application
/// blindly assume this to be true, which means using any one of those a select
/// could result in some interesting and unexpected errors. Avoid that by using
/// an fd that doesn't have a pre-determined usage.
const LOWEST_FD: libc::c_int = 3;
