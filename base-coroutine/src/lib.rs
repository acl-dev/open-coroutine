// export defer
pub use scopeguard::*;

pub use coroutine::*;
#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
pub use epoll::*;
pub use event_loop::event::*;
pub use event_loop::interest::*;
pub use event_loop::*;
pub use scheduler::*;
pub use stack::{Stack, StackError};
pub use work_steal_queue::*;

#[allow(dead_code)]
mod id;

#[allow(dead_code)]
mod stack;

#[allow(dead_code)]
mod context;

#[allow(dead_code)]
pub mod coroutine;

#[allow(dead_code)]
mod work_steal;

#[allow(dead_code)]
pub mod scheduler;

#[cfg(unix)]
#[macro_export]
macro_rules! shield {
    () => {{
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigaddset(&mut set, libc::SIGURG);
            let mut oldset: libc::sigset_t = std::mem::zeroed();
            libc::pthread_sigmask(libc::SIG_SETMASK, &set, &mut oldset);
            oldset
        }
    }};
}

#[cfg(unix)]
#[macro_export]
macro_rules! unbreakable {
    ( ( $fn: expr ) ( $($arg: expr),* $(,)* ) ) => {{
        unsafe {
            let mut oldset = $crate::shield!();
            let res = $fn($($arg, )*);
            libc::pthread_sigmask(libc::SIG_SETMASK, &oldset, std::ptr::null_mut());
            res
        }
    }};
}

#[allow(dead_code)]
mod monitor;

#[allow(dead_code)]
pub mod event_loop;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
#[allow(dead_code)]
pub mod epoll;
