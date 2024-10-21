use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn socket(
    fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
    domain: c_int,
    ty: c_int,
    protocol: c_int,
) -> c_int {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                SocketSyscallFacade<IoUringSocketSyscall<RawSocketSyscall>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<SocketSyscallFacade<RawSocketSyscall>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.socket(fn_ptr, domain, ty, protocol)
}

trait SocketSyscall {
    extern "C" fn socket(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, c_int, c_int) -> c_int>,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> c_int;
}

impl_facade!(SocketSyscallFacade, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);

impl_io_uring!(IoUringSocketSyscall, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);

impl_raw!(RawSocketSyscall, SocketSyscall,
    socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int
);
