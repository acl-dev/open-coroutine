use libc::{size_t, ssize_t};
use std::ffi::{c_int, c_void};

//防止重入：info!()/error!()内部会调用write()，如果write被hook了，
//会导致无限递归或嵌套状态转换。当检测到重入时，直接调用内部系统调用跳过facade逻辑。
// Re-entrancy guard: info!()/error!() internally call write(). If write is hooked,
// this causes infinite recursion or nested state transitions that corrupt coroutine state.
// When re-entrancy is detected, bypass the facade and call the inner syscall directly.
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

trait WriteSyscall {
    extern "C" fn write(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const c_void, size_t) -> ssize_t>,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
    ) -> ssize_t;
}

impl_syscall!(WriteSyscallFacade, IoUringWriteSyscall, NioWriteSyscall, RawWriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

//write的facade需要特殊处理：stdout/stderr的write由日志框架(tracing)触发，
//必须跳过状态转换和日志记录直接调用内层，否则facade内部的info!()会再次
//触发write导致stdout RefCell重复借用（无限递归）。
// The write facade needs special handling: writes to stdout/stderr are
// triggered by the logging framework (tracing). They must skip state
// transitions and logging, going directly to the inner layer. Otherwise
// the facade's info!() would re-trigger write, causing stdout's RefCell
// to be double-borrowed (infinite recursion).
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
        // stdout(1)/stderr(2)由日志框架触发，或已在facade内部（防重入），
        // 直接调用内层跳过状态转换和日志记录
        // Bypass state transitions for stdout/stderr (logging fds) and
        // when already inside a facade (re-entrancy guard)
        if fd == libc::STDOUT_FILENO
            || fd == libc::STDERR_FILENO
            || in_facade()
        {
            return self.inner.write(fn_ptr, fd, buf, len);
        }
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
