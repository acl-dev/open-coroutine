use libc::{sockaddr, socklen_t};
use std::ffi::c_int;

trait AcceptSyscall {
    extern "C" fn accept(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
        fd: c_int,
        address: *mut sockaddr,
        address_len: *mut socklen_t,
    ) -> c_int;
}

impl_syscall!(AcceptSyscallFacade, IoUringAcceptSyscall, NioAcceptSyscall, RawAcceptSyscall,
    accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int
);

impl_facade!(AcceptSyscallFacade, AcceptSyscall,
    accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int
);

impl_io_uring_read!(IoUringAcceptSyscall, AcceptSyscall,
    accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int
);

impl_nio_read!(NioAcceptSyscall, AcceptSyscall,
    accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int
);

impl_raw!(RawAcceptSyscall, AcceptSyscall,
    accept(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t) -> c_int
);
