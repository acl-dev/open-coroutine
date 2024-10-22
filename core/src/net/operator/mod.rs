use derivative::Derivative;
use io_uring::opcode::{
    Accept, AsyncCancel, Close, Connect, EpollCtl, Fsync, MkDirAt, OpenAt, PollAdd, PollRemove,
    Read, Readv, Recv, RecvMsg, RenameAt, Send, SendMsg, SendZc, Shutdown, Socket, Timeout,
    TimeoutRemove, TimeoutUpdate, Write, Writev,
};
use io_uring::squeue::Entry;
use io_uring::types::{epoll_event, Fd, Timespec};
use io_uring::{CompletionQueue, IoUring, Probe};
use libc::{
    c_char, c_int, c_uint, c_void, iovec, mode_t, msghdr, off_t, size_t, sockaddr, socklen_t, EBUSY,
};
use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::io::{Error, ErrorKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

static SUPPORT: Lazy<bool> =
    Lazy::new(|| crate::common::current_kernel_version() >= crate::common::kernel_version(5, 6, 0));

#[must_use]
pub(crate) fn support_io_uring() -> bool {
    *SUPPORT
}

static PROBE: Lazy<Probe> = Lazy::new(|| {
    let mut probe = Probe::new();
    if let Ok(io_uring) = IoUring::new(2) {
        if let Ok(()) = io_uring.submitter().register_probe(&mut probe) {
            return probe;
        }
    }
    panic!("probe init failed !")
});

// check https://www.rustwiki.org.cn/en/reference/introduction.html for help information
macro_rules! support {
    ( $self:ident, $struct_name:ident, $opcode:ident, $impls:expr ) => {
        return {
            static $struct_name: Lazy<bool> = once_cell::sync::Lazy::new(|| {
                if $crate::net::operator::support_io_uring() {
                    return PROBE.is_supported($opcode::CODE);
                }
                false
            });
            if *$struct_name {
                return $self.push_sq($impls);
            }
            Err(Error::new(ErrorKind::Unsupported, "unsupported"))
        }
    };
}

#[repr(C)]
#[derive(Derivative)]
#[derivative(Debug)]
pub(crate) struct Operator<'o> {
    #[derivative(Debug = "ignore")]
    inner: IoUring,
    entering: AtomicBool,
    backlog: Mutex<VecDeque<&'o Entry>>,
}

impl Operator<'_> {
    pub(crate) fn new(cpu: usize) -> std::io::Result<Self> {
        IoUring::builder()
            .setup_sqpoll(1000)
            .setup_sqpoll_cpu(u32::try_from(cpu).unwrap_or(u32::MAX))
            .build(1024)
            .map(|inner| Operator {
                inner,
                entering: AtomicBool::new(false),
                backlog: Mutex::new(VecDeque::new()),
            })
    }

    fn push_sq(&self, entry: Entry) -> std::io::Result<()> {
        let entry = Box::leak(Box::new(entry));
        if unsafe { self.inner.submission_shared().push(entry).is_err() } {
            self.backlog
                .lock()
                .expect("backlog lock failed")
                .push_back(entry);
        }
        self.inner.submit().map(|_| ())
    }

    pub(crate) fn select(
        &self,
        timeout: Option<Duration>,
        want: usize,
    ) -> std::io::Result<(usize, CompletionQueue, Option<Duration>)> {
        if support_io_uring() {
            if self
                .entering
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                return Ok((0, unsafe { self.inner.completion_shared() }, timeout));
            }
            let result = self.do_select(timeout, want);
            self.entering.store(false, Ordering::Release);
            return result;
        }
        Err(Error::new(ErrorKind::Unsupported, "unsupported"))
    }

    fn do_select(
        &self,
        timeout: Option<Duration>,
        want: usize,
    ) -> std::io::Result<(usize, CompletionQueue, Option<Duration>)> {
        let start_time = Instant::now();
        self.timeout_add(crate::common::constants::IO_URING_TIMEOUT_USERDATA, timeout)?;
        let mut cq = unsafe { self.inner.completion_shared() };
        // when submit queue is empty, submit_and_wait will block
        let count = match self.inner.submit_and_wait(want) {
            Ok(count) => count,
            Err(err) => {
                if err.raw_os_error() == Some(EBUSY) {
                    0
                } else {
                    return Err(err);
                }
            }
        };
        cq.sync();

        // clean backlog
        let mut sq = unsafe { self.inner.submission_shared() };
        loop {
            if sq.is_full() {
                match self.inner.submit() {
                    Ok(_) => (),
                    Err(err) => {
                        if err.raw_os_error() == Some(EBUSY) {
                            break;
                        }
                        return Err(err);
                    }
                }
            }
            sq.sync();

            let mut backlog = self.backlog.lock().expect("backlog lock failed");
            match backlog.pop_front() {
                Some(sqe) => {
                    if unsafe { sq.push(sqe).is_err() } {
                        backlog.push_front(sqe);
                        break;
                    }
                }
                None => break,
            }
        }
        let cost = Instant::now().saturating_duration_since(start_time);
        Ok((count, cq, timeout.map(|t| t.saturating_sub(cost))))
    }

    pub(crate) fn async_cancel(&self, user_data: usize) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_ASYNC_CANCEL,
            AsyncCancel,
            AsyncCancel::new(user_data as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn epoll_ctl(
        &self,
        user_data: usize,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut libc::epoll_event,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_EPOLL_CTL,
            EpollCtl,
            EpollCtl::new(
                Fd(epfd),
                Fd(fd),
                op,
                event.cast_const().cast::<epoll_event>(),
            )
            .build()
            .user_data(user_data as u64)
        )
    }

    pub(crate) fn poll_add(
        &self,
        user_data: usize,
        fd: c_int,
        flags: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_POLL_ADD,
            PollAdd,
            PollAdd::new(Fd(fd), flags as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn poll_remove(&self, user_data: usize) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_POLL_REMOVE,
            PollRemove,
            PollRemove::new(user_data as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn timeout_add(
        &self,
        user_data: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        if let Some(duration) = timeout {
            let timeout = Timespec::new()
                .sec(duration.as_secs())
                .nsec(duration.subsec_nanos());
            support!(
                self,
                SUPPORT_TIMEOUT_ADD,
                Timeout,
                Timeout::new(&timeout).build().user_data(user_data as u64)
            )
        }
        Ok(())
    }

    pub(crate) fn timeout_update(
        &self,
        user_data: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        if let Some(duration) = timeout {
            let timeout = Timespec::new()
                .sec(duration.as_secs())
                .nsec(duration.subsec_nanos());
            support!(
                self,
                SUPPORT_TIMEOUT_UPDATE,
                TimeoutUpdate,
                TimeoutUpdate::new(user_data as u64, &timeout)
                    .build()
                    .user_data(user_data as u64)
            )
        }
        self.timeout_remove(user_data)
    }

    pub(crate) fn timeout_remove(&self, user_data: usize) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_TIMEOUT_REMOVE,
            TimeoutRemove,
            TimeoutRemove::new(user_data as u64).build()
        )
    }

    pub(crate) fn openat(
        &self,
        user_data: usize,
        dir_fd: c_int,
        pathname: *const c_char,
        flags: c_int,
        mode: mode_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_OPENAT,
            OpenAt,
            OpenAt::new(Fd(dir_fd), pathname)
                .flags(flags)
                .mode(mode)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn mkdirat(
        &self,
        user_data: usize,
        dir_fd: c_int,
        pathname: *const c_char,
        mode: mode_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_MK_DIR_AT,
            MkDirAt,
            MkDirAt::new(Fd(dir_fd), pathname)
                .mode(mode)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn renameat(
        &self,
        user_data: usize,
        old_dir_fd: c_int,
        old_path: *const c_char,
        new_dir_fd: c_int,
        new_path: *const c_char,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_RENAME_AT,
            RenameAt,
            RenameAt::new(Fd(old_dir_fd), old_path, Fd(new_dir_fd), new_path)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn renameat2(
        &self,
        user_data: usize,
        old_dir_fd: c_int,
        old_path: *const c_char,
        new_dir_fd: c_int,
        new_path: *const c_char,
        flags: c_uint,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_RENAME_AT,
            RenameAt,
            RenameAt::new(Fd(old_dir_fd), old_path, Fd(new_dir_fd), new_path)
                .flags(flags)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn fsync(&self, user_data: usize, fd: c_int) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_FSYNC,
            Fsync,
            Fsync::new(Fd(fd)).build().user_data(user_data as u64)
        )
    }

    pub(crate) fn socket(
        &self,
        user_data: usize,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_SOCKET,
            Socket,
            Socket::new(domain, ty, protocol)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn accept(
        &self,
        user_data: usize,
        fd: c_int,
        address: *mut sockaddr,
        address_len: *mut socklen_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_ACCEPT,
            Accept,
            Accept::new(Fd(fd), address, address_len)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn accept4(
        &self,
        user_data: usize,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_ACCEPT,
            Accept,
            Accept::new(Fd(fd), addr, len)
                .flags(flg)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn connect(
        &self,
        user_data: usize,
        fd: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_CONNECT,
            Connect,
            Connect::new(Fd(fd), address, len)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn shutdown(&self, user_data: usize, fd: c_int, how: c_int) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_SHUTDOWN,
            Shutdown,
            Shutdown::new(Fd(fd), how)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn close(&self, user_data: usize, fd: c_int) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_CLOSE,
            Close,
            Close::new(Fd(fd)).build().user_data(user_data as u64)
        )
    }

    pub(crate) fn recv(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_RECV,
            Recv,
            Recv::new(Fd(fd), buf.cast::<u8>(), len as u32)
                .flags(flags)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn read(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_READ,
            Read,
            Read::new(Fd(fd), buf.cast::<u8>(), count as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn pread(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_READ,
            Read,
            Read::new(Fd(fd), buf.cast::<u8>(), count as u32)
                .offset(offset as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn readv(
        &self,
        user_data: usize,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_READV,
            Readv,
            Readv::new(Fd(fd), iov, iovcnt as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn preadv(
        &self,
        user_data: usize,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_READV,
            Readv,
            Readv::new(Fd(fd), iov, iovcnt as u32)
                .offset(offset as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn recvmsg(
        &self,
        user_data: usize,
        fd: c_int,
        msg: *mut msghdr,
        flags: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_RECVMSG,
            RecvMsg,
            RecvMsg::new(Fd(fd), msg)
                .flags(flags as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn send(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_SEND,
            Send,
            Send::new(Fd(fd), buf.cast::<u8>(), len as u32)
                .flags(flags)
                .build()
                .user_data(user_data as u64)
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sendto(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_SEND_ZC,
            SendZc,
            SendZc::new(Fd(fd), buf.cast::<u8>(), len as u32)
                .flags(flags)
                .dest_addr(addr)
                .dest_addr_len(addrlen)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn write(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_WRITE,
            Write,
            Write::new(Fd(fd), buf.cast::<u8>(), count as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn pwrite(
        &self,
        user_data: usize,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_WRITE,
            Write,
            Write::new(Fd(fd), buf.cast::<u8>(), count as u32)
                .offset(offset as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn writev(
        &self,
        user_data: usize,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_WRITEV,
            Writev,
            Writev::new(Fd(fd), iov, iovcnt as u32)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn pwritev(
        &self,
        user_data: usize,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_WRITEV,
            Writev,
            Writev::new(Fd(fd), iov, iovcnt as u32)
                .offset(offset as u64)
                .build()
                .user_data(user_data as u64)
        )
    }

    pub(crate) fn sendmsg(
        &self,
        user_data: usize,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> std::io::Result<()> {
        support!(
            self,
            SUPPORT_SENDMSG,
            SendMsg,
            SendMsg::new(Fd(fd), msg)
                .flags(flags as u32)
                .build()
                .user_data(user_data as u64)
        )
    }
}
