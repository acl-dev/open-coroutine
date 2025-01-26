use libc::off_t;
use std::ffi::c_int;

trait LseekSyscall {
    extern "C" fn lseek(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, off_t, c_int) -> off_t>,
        fd: c_int,
        offset: off_t,
        whence: c_int,
    ) -> off_t;
}

impl_syscall!(LseekSyscallFacade, RawLseekSyscall,
    lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t
);

impl_facade!(LseekSyscallFacade, LseekSyscall,
    lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t
);

impl_raw!(RawLseekSyscall, LseekSyscall,
    lseek(fd: c_int, offset: off_t, whence: c_int) -> off_t
);
