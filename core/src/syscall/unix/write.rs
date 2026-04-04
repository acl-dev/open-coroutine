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

impl_syscall!(WriteSyscallFacade, IoUringWriteSyscall, NioWriteSyscall, RawWriteSyscall,
    write(fd: c_int, buf: *const c_void, len: size_t) -> ssize_t
);

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
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            let new_state = crate::common::constants::SyscallState::Executing;
            if co.syscall((), syscall, new_state).is_err() {
                crate::error!("{} change to syscall {} {} failed !",
                    co.name(), syscall, new_state
                );
            }
        }
        if fd == libc::STDOUT_FILENO || fd == libc::STDERR_FILENO {
            return RawWriteSyscall::default().write(fn_ptr, fd, buf, len);
        }
        crate::info!("enter syscall {}", syscall);
        let r = self.inner.write(fn_ptr, fd, buf, len);
        if let Some(co) = crate::scheduler::SchedulableCoroutine::current() {
            if co.running().is_err() {
                crate::error!("{} change to running state failed !", co.name());
            }
        }
        crate::info!("exit syscall {} {:?} {}", syscall, r, std::io::Error::last_os_error());
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
