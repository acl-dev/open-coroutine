use libc::{size_t, ssize_t};
use std::ffi::{c_int, c_void};

trait WriteSyscall {
    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
    ) -> ssize_t;
}

//在最顶层对stdout/stderr/重入写入做早期旁路：直接调用原始系统调用，
//跳过整个facade链(WriteSyscallFacade/IoUring/NIO)，最小化每次info!()
//调用write()时的函数调用开销。在QEMU等慢速平台上，每个额外的函数调用
//可能耗时0.5-1ms，累积的开销会导致协程在10ms抢占窗口内无法完成工作。
// Early bypass at the top-level dispatcher for stdout/stderr/re-entrant writes:
// call the raw syscall directly, skipping the entire facade chain
// (WriteSyscallFacade/IoUring/NIO). This minimizes function call overhead
// per info!() → write() invocation. On slow platforms (QEMU), each extra
// function call can cost 0.5-1ms, and cumulative overhead prevents coroutines
// from completing work within the 10ms preemption window.
#[must_use]
pub extern "C" fn write(
    fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
    fd: c_int,
    buf: *const c_void,
    len: size_t,
) -> ssize_t {
    if fd == libc::STDOUT_FILENO || fd == libc::STDERR_FILENO {
        return RawWriteSyscall::default().write(fn_ptr, fd, buf, len);
    }
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: once_cell::sync::Lazy<
                WriteSyscallFacade<IoUringWriteSyscall<NioWriteSyscall<RawWriteSyscall>>>
            > = once_cell::sync::Lazy::new(Default::default);
        } else {
            static CHAIN: once_cell::sync::Lazy<WriteSyscallFacade<NioWriteSyscall<RawWriteSyscall>>> =
                once_cell::sync::Lazy::new(Default::default);
        }
    }
    CHAIN.write(fn_ptr, fd, buf, len)
}

impl_facade!(WriteSyscallFacade, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_io_uring_write!(IoUringWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_nio_write_buf!(NioWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_raw!(RawWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);
