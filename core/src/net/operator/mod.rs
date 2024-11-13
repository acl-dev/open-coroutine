#[allow(clippy::too_many_arguments)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "io_uring"))]
pub(crate) use linux::*;

#[allow(non_snake_case, clippy::too_many_arguments)]
#[cfg(all(windows, feature = "iocp"))]
mod windows;
#[cfg(all(windows, feature = "iocp"))]
pub(crate) use windows::*;
