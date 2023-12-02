#[macro_export]
macro_rules! impl_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut r;
        loop {
            r = $invoker.$syscall($fn_ptr, $socket, $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                break;
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                _ = $crate::net::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                );
            } else if error_kind != std::io::ErrorKind::Interrupted {
                break;
            }
        }
        if blocking {
            $crate::syscall::common::set_blocking(socket);
        }
        r
    }};
}
