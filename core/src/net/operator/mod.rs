#[allow(clippy::too_many_arguments)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "io_uring"))]
pub(crate) use linux::*;
