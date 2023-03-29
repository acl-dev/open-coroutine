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
#[cfg(feature = "stack-trace")]
pub use stack_trace::*;
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
pub mod scheduler;

#[cfg(unix)]
#[macro_export]
macro_rules! shield {
    () => {{
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::sigaddset(&mut set, $crate::monitor::Monitor::signum()),
                0
            );
            let mut oldset: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_SETMASK, &set, &mut oldset),
                0
            );
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

pub(crate) mod defer;

#[allow(dead_code)]
pub mod monitor;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
#[allow(dead_code)]
pub mod epoll;

#[allow(dead_code)]
pub mod event_loop;

#[allow(dead_code)]
#[cfg(feature = "stack-trace")]
pub mod stack_trace;
