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
pub use work_steal::*;

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
mod random;

#[allow(dead_code)]
pub mod scheduler;

#[allow(dead_code)]
#[cfg(unix)]
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
