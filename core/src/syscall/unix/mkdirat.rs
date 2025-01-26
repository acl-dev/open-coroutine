use libc::mode_t;
use std::ffi::{c_char, c_int};

trait MkdiratSyscall {
    extern "C" fn mkdirat(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_char, mode_t) -> c_int>,
        dirfd: c_int,
        pathname: *const c_char,
        mode: mode_t,
    ) -> c_int;
}

impl_syscall2!(MkdiratSyscallFacade, IoUringMkdiratSyscall, RawMkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);

impl_facade!(MkdiratSyscallFacade, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);

impl_io_uring!(IoUringMkdiratSyscall, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);

impl_raw!(RawMkdiratSyscall, MkdiratSyscall,
    mkdirat(dirfd: c_int, pathname: *const c_char, mode: mode_t) -> c_int
);
