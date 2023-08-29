use once_cell::sync::Lazy;
use std::ffi::c_int;

extern "C" {
    fn linux_version_code() -> c_int;
    fn linux_version_major() -> c_int;
    fn linux_version_patchlevel() -> c_int;
    fn linux_version_sublevel() -> c_int;
}

#[must_use]
pub fn kernel_version(major: c_int, patchlevel: c_int, sublevel: c_int) -> c_int {
    ((major) << 16) + ((patchlevel) << 8) + if (sublevel) > 255 { 255 } else { sublevel }
}

#[must_use]
pub fn current_kernel_version() -> c_int {
    unsafe { linux_version_code() }
}

#[must_use]
pub fn current_kernel_major() -> c_int {
    unsafe { linux_version_major() }
}

#[must_use]
pub fn current_kernel_patchlevel() -> c_int {
    unsafe { linux_version_patchlevel() }
}

#[must_use]
pub fn current_kernel_sublevel() -> c_int {
    unsafe { linux_version_sublevel() }
}

static SUPPORT: Lazy<bool> = Lazy::new(|| current_kernel_version() >= kernel_version(5, 6, 0));

#[must_use]
pub fn support_io_uring() -> bool {
    *SUPPORT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        println!("{}", current_kernel_version());
        println!(
            "{}.{}.{}",
            current_kernel_major(),
            current_kernel_patchlevel(),
            current_kernel_sublevel()
        );
        println!("{}", support_io_uring());
    }
}
