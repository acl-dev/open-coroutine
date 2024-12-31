use libc::{sockaddr, socklen_t};
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn accept(
    fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
    fd: c_int,
    address: *mut sockaddr,
    address_len: *mut socklen_t,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                AcceptSyscallFacade<IoUringAcceptSyscall<NioAcceptSyscall<RawAcceptSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<AcceptSyscallFacade<NioAcceptSyscall<RawAcceptSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.accept(fn_ptr, fd, address, address_len)
}

trait AcceptSyscall {
    extern "C" fn accept(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int>,
        fd: c_int,
        address: *mut sockaddr,
        address_len: *mut socklen_t,
    ) -> c_int;
}

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
