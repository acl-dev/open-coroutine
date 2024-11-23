use crate::common::now;
use crate::net::EventLoops;
use crate::syscall::{is_blocking, reset_errno, set_blocking, set_non_blocking, recv_time_limit};
use libc::{msghdr, ssize_t};
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_void};
use std::io::{Error, ErrorKind};

#[must_use]
pub extern "C" fn recvmsg(
    fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
    fd: c_int,
    msg: *mut msghdr,
    flags: c_int,
) -> ssize_t {
    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
            static CHAIN: Lazy<
                RecvmsgSyscallFacade<IoUringRecvmsgSyscall<NioRecvmsgSyscall<RawRecvmsgSyscall>>>
            > = Lazy::new(Default::default);
        } else {
            static CHAIN: Lazy<RecvmsgSyscallFacade<NioRecvmsgSyscall<RawRecvmsgSyscall>>> =
                Lazy::new(Default::default);
        }
    }
    CHAIN.recvmsg(fn_ptr, fd, msg, flags)
}

trait RecvmsgSyscall {
    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t;
}

impl_facade!(RecvmsgSyscallFacade, RecvmsgSyscall,
    recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t
);

impl_io_uring!(IoUringRecvmsgSyscall, RecvmsgSyscall,
    recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioRecvmsgSyscall<I: RecvmsgSyscall> {
    inner: I,
}

impl<I: RecvmsgSyscall> RecvmsgSyscall for NioRecvmsgSyscall<I> {
    #[allow(clippy::too_many_lines)]
    extern "C" fn recvmsg(
        &self,
        fn_ptr: Option<&extern "C" fn(c_int, *mut msghdr, c_int) -> ssize_t>,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> ssize_t {
        let blocking = is_blocking(fd);
        if blocking {
            set_non_blocking(fd);
        }
        let start_time = now();
        let mut left_time = recv_time_limit(fd);
        let msghdr = unsafe { *msg };
        let vec = unsafe {
            Vec::from_raw_parts(
                msghdr.msg_iov,
                msghdr.msg_iovlen.try_into().expect("overflow"),
                msghdr.msg_iovlen.try_into().expect("overflow"),
            )
        };
        let mut length = 0;
        let mut received = 0usize;
        let mut r = 0;
        let mut index = 0;
        for iovec in &vec {
            let mut offset = received.saturating_sub(length);
            length += iovec.iov_len;
            if received > length {
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
            while received < length && left_time > 0 {
                if 0 != offset {
                    iov[0] = libc::iovec {
                        iov_base: (iov[0].iov_base as usize + offset) as *mut c_void,
                        iov_len: iov[0].iov_len - offset,
                    };
                }
                let mut arg = msghdr {
                    msg_name: msghdr.msg_name,
                    msg_namelen: msghdr.msg_namelen,
                    msg_iov: iov.as_mut_ptr(),
                    msg_iovlen,
                    msg_control: msghdr.msg_control,
                    msg_controllen: msghdr.msg_controllen,
                    msg_flags: msghdr.msg_flags,
                };
                r = self.inner.recvmsg(fn_ptr, fd, &mut arg, flags);
                if r == 0 {
                    std::mem::forget(vec);
                    if blocking {
                        set_blocking(fd);
                    }
                    return r;
                } else if r != -1 {
                    reset_errno();
                    received += libc::size_t::try_from(r).expect("r overflow");
                    if received >= length {
                        r = received.try_into().expect("received overflow");
                        break;
                    }
                    offset = received.saturating_sub(length);
                }
                let error_kind = Error::last_os_error().kind();
                if error_kind == ErrorKind::WouldBlock {
                    //wait read event
                    left_time = start_time
                        .saturating_add(recv_time_limit(fd))
                        .saturating_sub(now());
                    let wait_time = std::time::Duration::from_nanos(left_time)
                        .min(crate::common::constants::SLICE);
                    if EventLoops::wait_read_event(fd, Some(wait_time)).is_err() {
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
            if received >= length {
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

impl_raw!(RawRecvmsgSyscall, RecvmsgSyscall,
    recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t
);
