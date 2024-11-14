use crate::impl_display_by_debug;
use once_cell::sync::Lazy;
use std::time::Duration;

/// Recommended stack size for coroutines.
pub const DEFAULT_STACK_SIZE: usize = 128 * 1024;

/// A user data used to indicate the timeout of `io_uring_enter`.
#[cfg(all(target_os = "linux", feature = "io_uring"))]
pub const IO_URING_TIMEOUT_USERDATA: usize = usize::MAX - 1;

/// Coroutine global queue bean name.
pub const COROUTINE_GLOBAL_QUEUE_BEAN: &str = "coroutineGlobalQueueBean";

/// Task global queue bean name.
pub const TASK_GLOBAL_QUEUE_BEAN: &str = "taskGlobalQueueBean";

/// Monitor bean name.
pub const MONITOR_BEAN: &str = "monitorBean";

/// Default time slice.
pub const SLICE: Duration = Duration::from_millis(10);

/// Get the cpu count
#[must_use]
pub fn cpu_count() -> usize {
    static CPU_COUNT: Lazy<usize> = Lazy::new(num_cpus::get);
    *CPU_COUNT
}

/// Enums used to describe pool state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PoolState {
    /// The pool is running.
    Running,
    /// The pool is stopping.
    Stopping,
    /// The pool is stopped.
    Stopped,
}

impl_display_by_debug!(PoolState);

/// Enums used to describe syscall
#[allow(non_camel_case_types, missing_docs)]
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Syscall {
    #[cfg(windows)]
    Sleep,
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
    setsockopt,
    recv,
    #[cfg(windows)]
    WSARecv,
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
    #[cfg(windows)]
    WSASocketW,
    #[cfg(windows)]
    ioctlsocket,
    send,
    #[cfg(windows)]
    WSASend,
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
    mkdir,
    mkdirat,
    rmdir,
    lseek,
    openat,
    link,
    unlink,
    pthread_cond_timedwait,
    pthread_mutex_trylock,
    pthread_mutex_lock,
    pthread_mutex_unlock,
    #[cfg(windows)]
    CreateFileW,
    #[cfg(windows)]
    SetFilePointerEx,
    #[cfg(windows)]
    WaitOnAddress,
    #[cfg(windows)]
    WSAPoll,
}

impl Syscall {
    /// Get the `NIO` syscall.
    #[must_use]
    pub fn nio() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {
                Self::epoll_wait
            } else if #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd"
            ))] {
                Self::kevent
            } else if #[cfg(windows)] {
                Self::iocp
            } else {
                compile_error!("unsupported")
            }
        }
    }
}

impl_display_by_debug!(Syscall);

impl From<Syscall> for &str {
    fn from(val: Syscall) -> Self {
        format!("{val}").leak()
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
    ///系统调用回调成功
    Callback,
}

impl_display_by_debug!(SyscallState);

/// Enums used to describe coroutine state
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState<Y, R> {
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

impl_display_by_debug!(CoroutineState<Y, R>);
