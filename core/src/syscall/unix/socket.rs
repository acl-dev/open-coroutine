use std::ffi::c_int;

trait SocketSyscall {
    extern "C" fn socket(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> c_int;
}

impl_syscall2!(SocketSyscallFacade, IoUringSocketSyscall, RawSocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);

impl_facade!(SocketSyscallFacade, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);

impl_io_uring!(IoUringSocketSyscall, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);

impl_raw!(RawSocketSyscall, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);
