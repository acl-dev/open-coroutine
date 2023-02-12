// export defer
pub use scopeguard::*;
pub use work_steal_queue::*;

#[allow(dead_code)]
pub mod coroutine;
pub use coroutine::*;

#[allow(dead_code)]
pub mod scheduler;
pub use scheduler::*;

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
    ( $fn: expr ) => {{
        let oldset = $crate::shield!();
        unsafe {
            let res = $fn;
            libc::pthread_sigmask(libc::SIG_SETMASK, &oldset, std::ptr::null_mut());
            res
        }
    }};
}

#[allow(dead_code)]
mod monitor;

#[allow(dead_code)]
pub mod event_loop;
pub use event_loop::event::*;
pub use event_loop::interest::*;
pub use event_loop::*;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
#[allow(dead_code)]
pub mod epoll;
#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
pub use epoll::*;
