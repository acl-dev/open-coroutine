use crate::common::now;
use crate::net::EventLoops;
use crate::syscall::common::{is_blocking, reset_errno, set_blocking, set_non_blocking, send_time_limit};
use libc::{msghdr, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};
use std::io::{Error, ErrorKind};

#[must_use]
pub extern "C" fn sendmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *const msghdr,
    flags: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                SendmsgSyscallFacade<IoUringSendmsgSyscall<NioSendmsgSyscall<RawSendmsgSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<SendmsgSyscallFacade<NioSendmsgSyscall<RawSendmsgSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.sendmsg(fn_ptr, fd, msg, flags)
}

trait SendmsgSyscall {
    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t;
}

impl_facade!(SendmsgSyscallFacade, SendmsgSyscall,
    sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t
);

impl_io_uring!(IoUringSendmsgSyscall, SendmsgSyscall,
    sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioSendmsgSyscall<I: SendmsgSyscall> {
    inner: I,
}

impl<I: SendmsgSyscall> SendmsgSyscall for NioSendmsgSyscall<I> {
    #[allow(clippy::too_many_lines)]
    extern "C" fn sendmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *const msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> ssize_t {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let start_time = now();
        let mut left_time = start_time
            .saturating_add(send_time_limit(fd))
            .saturating_sub(start_time);
        let msghdr = unsafe { *msg };
        let vec = unsafe {
            Vec::from_raw_parts(
                msghdr.msg_iov,
                msghdr.msg_iovlen as usize,
                msghdr.msg_iovlen as usize,
            )
        };
        let mut length = 0;
        let mut sent = 0usize;
        let mut r = 0;
        let mut index = 0;
        for iovec in &vec {
            let mut offset = sent.saturating_sub(length);
            length += iovec.iov_len;
            if sent > length {
                index += 1;
                continue;
            }
            let mut iov = Vec::new();
            for i in vec.iter().skip(index) {
                iov.push(*i);
            }
            cfg_if::cfg_if! {
                if #[cfg(any(
                    target_os = "linux",
                    target_os = "l4re",
                    target_os = "android",
                    target_os = "emscripten"
                ))] {
                    let msg_iovlen = vec.len();
                } else {
                    let msg_iovlen = c_int::try_from(iov.len()).unwrap_or_else(|_| {
                        panic!("{} msghdr.msg_iovlen overflow", crate::common::constants::Syscall::recvmsg)
                    });
                }
            }
            while sent < length && left_time > 0 {
                if 0 != offset {
                    iov[0] = libc::iovec {
                        iov_base: (iov[0].iov_base as usize + offset) as *mut c_void,
                        iov_len: iov[0].iov_len - offset,
                    };
                }
                let arg = msghdr {
                    msg_name: msghdr.msg_name,
                    msg_namelen: msghdr.msg_namelen,
                    msg_iov: iov.as_mut_ptr(),
                    msg_iovlen,
                    msg_control: msghdr.msg_control,
                    msg_controllen: msghdr.msg_controllen,
                    msg_flags: msghdr.msg_flags,
                };
                r = self.inner.sendmsg(fn_ptr, fd, &arg, flags);
                if r != -1 {
                    reset_errno();
                    sent += r as usize;
                    if sent >= length {
                        r = sent as ssize_t;
                        break;
                    }
                    offset = sent.saturating_sub(length);
                }
                let error_kind = Error::last_os_error().kind();
                if error_kind == ErrorKind::WouldBlock {
                    //wait write event
                    left_time = start_time
                        .saturating_add(send_time_limit(fd))
                        .saturating_sub(now());
                    let wait_time = std::time::Duration::from_nanos(left_time)
                        .min(crate::common::constants::SLICE);
                    if EventLoops::wait_write_event(fd, Some(wait_time)).is_err() {
                        std::mem::forget(vec);
                        if blocking {
                            set_blocking(fd);
                        }
                        return r;
                    }
                } else if error_kind != ErrorKind::Interrupted {
                    std::mem::forget(vec);
                    if blocking {
                        set_blocking(fd);
                    }
                    return r;
                }
            }
            if sent >= length {
                index += 1;
            }
        }
        std::mem::forget(vec);
        if blocking {
            set_blocking(fd);
        }
        r
    }
}

impl_raw!(RawSendmsgSyscall, SendmsgSyscall,
    sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t
);
