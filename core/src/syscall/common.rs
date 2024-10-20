pub use crate::syscall::{is_blocking, is_non_blocking, set_blocking, set_errno, set_non_blocking};

pub extern "C" fn reset_errno() {
    set_errno(0);
}

#[macro_export]
macro_rules! log_syscall {
    ( $socket:expr, $done:expr, $once_result:expr ) => {
        #[cfg(feature = "logs")]
        if let Some(coroutine) = $crate::scheduler::SchedulableCoroutine::current() {
            $crate::info!(
                "{} {} {} {} {} {}",
                coroutine.get_name(),
                coroutine.state(),
                $socket,
                $done,
                $once_result,
                std::io::Error::last_os_error(),
            );
        }
    };
}

#[macro_export]
macro_rules! impl_non_blocking {
    ( $socket:expr, $impls:expr ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let r = $impls;
        if blocking {
            $crate::syscall::common::set_blocking(socket);
        }
        return r;
    }};
}
