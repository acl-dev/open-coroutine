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
    if fd == libc::STDOUT_FILENO || fd == libc::STDERR_FILENO || in_facade() {
        if let Some(f) = fn_ptr {
            return (f)(fd, buf, len);
        }
        return unsafe { libc::write(fd, buf, len) };
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

//防止重入：info!()/error!()内部会调用write()，如果write被hook了，
//会导致无限递归或嵌套状态转换。当检测到重入时，直接调用原始系统调用跳过
//所有中间层(io_uring/NIO)，避免io_uring提交导致condvar死锁。
// Re-entrancy guard: info!()/error!() internally call write(). If write is hooked,
// this causes infinite recursion or nested state transitions that corrupt coroutine state.
// When re-entrancy is detected, bypass ALL layers (io_uring, NIO, facade) and call
// the raw syscall directly to avoid io_uring submission deadlocks.
thread_local! {
    static IN_FACADE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[inline]
fn in_facade() -> bool {
    IN_FACADE.get()
}

#[inline]
fn set_in_facade(val: bool) {
    IN_FACADE.set(val);
}

//write的facade需要特殊处理：stdout/stderr的write由日志框架(tracing)触发，
//必须跳过所有中间层(facade/io_uring/NIO)直接调用原始系统调用，否则：
//1. facade内部的info!()会再次触发write导致stdout RefCell重复借用（无限递归）
//2. io_uring层会提交写操作并阻塞在condvar等待完成，导致死锁
// The write facade needs special handling: writes to stdout/stderr are
// triggered by the logging framework (tracing). They must bypass ALL layers
// (facade, io_uring, NIO) and call the raw syscall directly. Otherwise:
// 1. The facade's info!() re-triggers write → stdout RefCell double-borrow
// 2. The io_uring layer submits the write and blocks on condvar → deadlock
#[repr(C)]
#[derive(Debug, Default)]
struct WriteSyscallFacade<I: WriteSyscall> {
    inner: I,
}

impl<I: WriteSyscall> WriteSyscall for WriteSyscallFacade<I> {
    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
    ) -> ssize_t {
        let syscall = crate::common::constants::SyscallName::write;
        set_in_facade(true);
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            let new_state = crate::common::constants::SyscallState::Executing;
            if co.syscall((), syscall, new_state).is_err() {
                crate::error!("{} change to syscall {} {} failed !",
                    co.name(), syscall, new_state
                );
            }
        }
        crate::info!("enter syscall {}", syscall);
        set_in_facade(false);
        let r = self.inner.write(fn_ptr, fd, buf, len);
        set_in_facade(true);
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            if co.running().is_err() {
                crate::error!("{} change to running state failed !", co.name());
            }
        }
        crate::info!("exit syscall {} {:?} {}", syscall, r, std::io::Error::last_os_error());
        set_in_facade(false);
        r
    }
}

impl_io_uring_write!(IoUringWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_nio_write_buf!(NioWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

impl_raw!(RawWriteSyscall, WriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);
