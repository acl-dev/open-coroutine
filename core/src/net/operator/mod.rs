#[cfg(all(target_os = "linux", feature = "io_uring"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "io_uring"))]
pub(crate) use linux::*;

#[cfg(all(windows, feature = "iocp"))]
mod windows;
#[cfg(all(windows, feature = "iocp"))]
pub(crate) use windows::*;
