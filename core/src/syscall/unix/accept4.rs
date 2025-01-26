use libc::{sockaddr, socklen_t};
use std::ffi::c_int;

trait Accept4Syscall {
    extern "C" fn accept4(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t, c_int) -> c_int>,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> c_int;
}

impl_syscall!(Accept4SyscallFacade, IoUringAccept4Syscall, NioAccept4Syscall, RawAccept4Syscall,
    accept4(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t, flg: c_int) -> c_int
);

impl_facade!(Accept4SyscallFacade, Accept4Syscall,
    accept4(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t, flg: c_int) -> c_int
);

impl_io_uring!(IoUringAccept4Syscall, Accept4Syscall,
    accept4(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t, flg: c_int) -> c_int
);

impl_nio_read!(NioAccept4Syscall, Accept4Syscall,
    accept4(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t, flg: c_int) -> c_int
);

impl_raw!(RawAccept4Syscall, Accept4Syscall,
    accept4(fd: c_int, address: *mut sockaddr, address_len: *mut socklen_t, flg: c_int) -> c_int
);
