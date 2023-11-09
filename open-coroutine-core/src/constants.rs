use std::fmt::{Debug, Display, Formatter};

/// min stack size for backtrace
pub const DEFAULT_STACK_SIZE: usize = 64 * 1024;

/// CPU bound to monitor
pub const MONITOR_CPU: usize = 0;

/// Enums used to describe syscall
#[allow(non_camel_case_types, missing_docs)]
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Syscall {
    sleep,
    usleep,
    nanosleep,
    poll,
    select,
    #[cfg(target_os = "linux")]
    accept4,
    #[cfg(target_os = "linux")]
    epoll_ctl,
    #[cfg(target_os = "linux")]
    epoll_wait,
    #[cfg(target_os = "linux")]
    io_uring_enter,
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    kevent,
    #[cfg(windows)]
    iocp,
    recv,
    recvfrom,
    read,
    pread,
    readv,
    preadv,
    recvmsg,
    connect,
    listen,
    accept,
    shutdown,
    close,
    socket,
    send,
    sendto,
    write,
    pwrite,
    writev,
    pwritev,
    sendmsg,
    fsync,
    renameat,
    #[cfg(target_os = "linux")]
    renameat2,
    mkdirat,
    openat,
}

impl Display for Syscall {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

thread_local! {
    static SYSCALL: std::cell::RefCell<std::collections::VecDeque<Syscall>> = std::cell::RefCell::new(std::collections::VecDeque::new());
}

#[allow(missing_docs)]
impl Syscall {
    pub fn init_current(current: Self) {
        SYSCALL.with(|s| {
            s.borrow_mut().push_front(current);
        });
    }

    #[must_use]
    pub fn current() -> Option<Self> {
        SYSCALL.with(|s| s.borrow().front().copied())
    }

    pub fn clean_current() {
        SYSCALL.with(|s| _ = s.borrow_mut().pop_front());
    }
}

/// Enums used to describe syscall state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SyscallState {
    ///执行中
    Executing,
    ///被挂起到指定时间后继续执行，参数为时间戳
    Suspend(u64),
    ///到指定时间戳后回来，期间系统调用可能没执行完毕
    ///对于sleep系列，这个状态表示正常完成
    Timeout,
    ///系统调用完成
    Finished,
}

impl Display for SyscallState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

/// Enums used to describe coroutine state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState<Y, R>
where
    Y: Copy + Eq + PartialEq,
    R: Copy + Eq + PartialEq,
{
    ///The coroutine is created.
    Created,
    ///The coroutine is ready to run.
    Ready,
    ///The coroutine is running.
    Running,
    ///The coroutine resumes execution after the specified time has been suspended(with a given value).
    Suspend(Y, u64),
    ///The coroutine enters the system call.
    SystemCall(Y, Syscall, SyscallState),
    /// The coroutine completed with a return value.
    Complete(R),
    /// The coroutine completed with a error message.
    Error(&'static str),
}

impl<Y, R> Display for CoroutineState<Y, R>
where
    Y: Copy + Eq + PartialEq + Debug,
    R: Copy + Eq + PartialEq + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

/// Enums used to describe pool state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PoolState {
    ///The pool is created.
    Created,
    ///The pool is running in an additional thread.
    Running,
    ///The pool is stopping, `true` means thread mode.
    Stopping(bool),
    ///The pool is stopped.
    Stopped,
}

impl Display for PoolState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
