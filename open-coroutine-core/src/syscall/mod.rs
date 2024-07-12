#[cfg(unix)]
pub mod common;

#[cfg(unix)]
pub use unix::*;

#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use windows::*;

#[allow(non_snake_case, dead_code)]
#[cfg(windows)]
mod windows;
