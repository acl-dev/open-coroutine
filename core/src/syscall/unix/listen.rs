use std::ffi::c_int;

trait ListenSyscall {
    extern "C" fn listen(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int) -> c_int>,
        fd: c_int,
        backlog: c_int,
    ) -> c_int;
}

impl_syscall!(ListenSyscallFacade, RawListenSyscall, listen(fd: c_int, backlog: c_int) -> c_int);

impl_facade!(ListenSyscallFacade, ListenSyscall, listen(fd: c_int, backlog: c_int) -> c_int);

impl_raw!(RawListenSyscall, ListenSyscall, listen(fd: c_int, backlog: c_int) -> c_int);
