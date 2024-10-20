pub mod common;

#[cfg(unix)]
pub use unix::*;

#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use windows::*;

#[allow(non_snake_case)]
#[cfg(windows)]
mod windows;
