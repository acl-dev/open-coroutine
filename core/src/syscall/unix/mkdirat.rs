use libc::mode_t;
use once_cell::sync::Lazy;
use std::ffi::{c_char, c_int};

#[must_use]
pub extern "C" fn mkdirat(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_char, mode_t) -> c_int>,
    dirfd: c_int,
    pathname: *const c_char,
    mode: mode_t,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<MkdiratSyscallFacade<IoUringMkdiratSyscall<RawMkdiratSyscall>>> =
                Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<MkdiratSyscallFacade<RawMkdiratSyscall>> = Lazy::new(Default::default);
        }
    }
    CHAIN.mkdirat(fn_ptr, dirfd, pathname, mode)
}

trait MkdiratSyscall {
    extern "C" fn mkdirat(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_char, mode_t) -> c_int>,
        dirfd: c_int,
        pathname: *const c_char,
        mode: mode_t,
    ) -> c_int;
}

impl_facade!(MkdiratSyscallFacade, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);

impl_io_uring!(IoUringMkdiratSyscall, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);

impl_raw!(RawMkdiratSyscall, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);
