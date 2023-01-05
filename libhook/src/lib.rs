#[allow(dead_code)]
mod common;

pub use common::*;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
mod epoll;

#[cfg(any(
    target_os = "linux",
    target_os = "l4re",
    target_os = "android",
    target_os = "emscripten"
))]
pub use epoll::*;

#[allow(dead_code)]
mod event_loop;

#[allow(dead_code, clippy::not_unsafe_ptr_arg_deref)]
#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::*;

#[allow(dead_code)]
#[cfg(all(windows, nightly))]
mod windows;

#[cfg(all(windows, nightly))]
pub use windows::*;
