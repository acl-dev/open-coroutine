#[allow(dead_code)]
mod common;

pub use common::*;

#[allow(dead_code)]
#[cfg(unix)]
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
