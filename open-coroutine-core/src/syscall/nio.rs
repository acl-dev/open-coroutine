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

#[macro_export]
macro_rules! impl_expected_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut received = 0;
        let mut r = 0;
        while received < $length {
            r = $invoker.$syscall(
                $fn_ptr,
                $socket,
                ($buffer as usize + received) as *mut c_void,
                $length - received,
                $($arg, )*
            );
            if r != -1 {
                $crate::syscall::common::reset_errno();
                received += r as size_t;
                if received >= $length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if $crate::net::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
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

#[macro_export]
macro_rules! impl_expected_batch_read_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov as *mut iovec, $length as usize, $length as usize)
        });
        let mut length = 0;
        let mut pices = std::collections::VecDeque::new();
        for iovec in &vec {
            length += iovec.iov_len;
            pices.push_back(length);
        }
        let mut received = 0;
        let mut r = 0;
        while received < length {
            // find from-index
            let mut from_index = 0;
            for (i, v) in pices.iter().enumerate() {
                if received < *v {
                    from_index = i;
                    break;
                }
            }
            // calculate offset
            let current_received_offset = if from_index > 0 {
                received.saturating_sub(pices[from_index.saturating_sub(1)])
            } else {
                received
            };
            // remove already received
            for _ in 0..from_index {
                _ = vec.pop_front();
                _ = pices.pop_front();
            }
            // build syscall args
            vec[0] = iovec {
                iov_base: (vec[0].iov_base as usize + current_received_offset) as *mut c_void,
                iov_len: vec[0].iov_len - current_received_offset,
            };
            r = $invoker.$syscall($fn_ptr, $socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                received += r as usize;
                if received >= length || r == 0 {
                    r = received as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait read event
                if $crate::net::event_loop::EventLoops::wait_read_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
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

#[macro_export]
macro_rules! impl_expected_write_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $buffer:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < $length {
            r = $invoker.$syscall(
                $fn_ptr,
                $socket,
                ($buffer as usize + sent) as *const c_void,
                $length - sent,
                $($arg, )*
            );
            if r != -1 {
                $crate::syscall::common::reset_errno();
                sent += r as size_t;
                if sent >= $length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if $crate::net::event_loop::EventLoops::wait_write_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
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

#[macro_export]
macro_rules! impl_expected_batch_write_hook {
    ( $invoker: expr, $syscall: ident, $fn_ptr: expr, $socket:expr, $iov:expr, $length:expr, $($arg: expr),* $(,)* ) => {{
        let socket = $socket;
        let blocking = $crate::syscall::common::is_blocking(socket);
        if blocking {
            $crate::syscall::common::set_non_blocking(socket);
        }
        let mut vec = std::collections::VecDeque::from(unsafe {
            Vec::from_raw_parts($iov as *mut iovec, $length as usize, $length as usize)
        });
        let mut length = 0;
        let mut pices = std::collections::VecDeque::new();
        for iovec in &vec {
            length += iovec.iov_len;
            pices.push_back(length);
        }
        let mut sent = 0;
        let mut r = 0;
        while sent < length {
            // find from-index
            let mut from_index = 0;
            for (i, v) in pices.iter().enumerate() {
                if sent < *v {
                    from_index = i;
                    break;
                }
            }
            // calculate offset
            let current_sent_offset = if from_index > 0 {
                sent.saturating_sub(pices[from_index.saturating_sub(1)])
            } else {
                sent
            };
            // remove already sent
            for _ in 0..from_index {
                _ = vec.pop_front();
                _ = pices.pop_front();
            }
            // build syscall args
            vec[0] = iovec {
                iov_base: (vec[0].iov_base as usize + current_sent_offset) as *mut c_void,
                iov_len: vec[0].iov_len - current_sent_offset,
            };
            r = $invoker.$syscall($fn_ptr, $socket, vec.get(0).unwrap(), c_int::try_from(vec.len()).unwrap(), $($arg, )*);
            if r != -1 {
                $crate::syscall::common::reset_errno();
                sent += r as usize;
                if sent >= length {
                    r = sent as ssize_t;
                    break;
                }
            }
            let error_kind = std::io::Error::last_os_error().kind();
            if error_kind == std::io::ErrorKind::WouldBlock {
                //wait write event
                if $crate::net::event_loop::EventLoops::wait_write_event(
                    socket,
                    Some(std::time::Duration::from_millis(10)),
                )
                .is_err()
                {
                    break;
                }
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
