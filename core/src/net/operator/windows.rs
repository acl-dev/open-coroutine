use crate::common::constants::Syscall;
use crate::common::{get_timeout_time, now};
use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    ERROR_NETNAME_DELETED, FALSE, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Networking::WinSock::{
    closesocket, WSAGetLastError, INVALID_SOCKET, IPPROTO, SOCKADDR, SOCKADDR_IN, SOCKET,
    WINSOCK_SOCKET_TYPE, WSA_FLAG_OVERLAPPED, WSA_IO_PENDING,
};
use windows_sys::Win32::System::IO::{
    CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED,
};

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct SocketContext {
    pub(crate) domain: c_int,
    pub(crate) ty: WINSOCK_SOCKET_TYPE,
    pub(crate) protocol: IPPROTO,
}

pub(crate) static SOCKET_CONTEXT: Lazy<DashMap<SOCKET, SocketContext>> =
    Lazy::new(Default::default);

/// The overlapped struct we actually used for IOCP.
#[repr(C)]
pub(crate) struct Overlapped {
    /// The base [`OVERLAPPED`].
    pub base: OVERLAPPED,
    pub from_fd: SOCKET,
    pub socket: SOCKET,
    pub token: usize,
    pub syscall: Syscall,
    pub dw_number_of_bytes_transferred: u32,
}

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Operator<'o> {
    iocp: HANDLE,
    entering: AtomicBool,
    handles: DashSet<HANDLE>,
    phantom_data: PhantomData<&'o HANDLE>,
}

impl Operator<'_> {
    pub(crate) fn new(_cpu: usize) -> std::io::Result<Self> {
        let iocp =
            unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 0) };
        if iocp == std::ptr::null_mut() {
            return Err(Error::last_os_error());
        }
        Ok(Self {
            iocp,
            entering: AtomicBool::new(false),
            handles: Default::default(),
            phantom_data: Default::default(),
        })
    }

    /// Associates a new `HANDLE` to this I/O completion port.
    ///
    /// This function will associate the given handle to this port with the
    /// given `token` to be returned in status messages whenever it receives a
    /// notification.
    ///
    /// Any object which is convertible to a `HANDLE` via the `AsRawHandle`
    /// trait can be provided to this function, such as `std::fs::File` and
    /// friends.
    fn add_handle(&self, token: usize, handle: HANDLE) -> std::io::Result<()> {
        assert_eq!(size_of_val(&token), size_of::<usize>());
        let ret = unsafe { CreateIoCompletionPort(handle, self.iocp, token, 0) };
        if ret == std::ptr::null_mut() {
            return Err(Error::last_os_error());
        }
        debug_assert_eq!(ret, self.iocp);
        Ok(())
    }

    pub(crate) fn select(
        &self,
        timeout: Option<Duration>,
        want: usize,
    ) -> std::io::Result<(usize, Vec<Overlapped>, Option<Duration>)> {
        if self
            .entering
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok((0, Vec::new(), timeout));
        }
        let result = self.do_select(timeout, want);
        self.entering.store(false, Ordering::Release);
        return result;
    }

    fn do_select(
        &self,
        timeout: Option<Duration>,
        want: usize,
    ) -> std::io::Result<(usize, Vec<Overlapped>, Option<Duration>)> {
        let start_time = Instant::now();
        let timeout_time = timeout.map(|t| get_timeout_time(t)).unwrap_or(u64::MAX);
        let mut cq = Vec::new();
        loop {
            let mut bytes = 0;
            let mut token = 0;
            let mut overlapped: Overlapped = unsafe { std::mem::zeroed() };
            let ret = unsafe {
                GetQueuedCompletionStatus(
                    self.iocp,
                    &mut bytes,
                    &mut token,
                    (&mut overlapped as *mut Overlapped).cast(),
                    1,
                )
            };
            if ret == FALSE {
                let err = Error::last_os_error().raw_os_error();
                if Some(ERROR_NETNAME_DELETED as i32) == err || Some(WAIT_TIMEOUT as i32) == err {
                    _ = unsafe { closesocket(overlapped.socket) };
                    if cq.len() >= want || timeout_time.saturating_sub(now()) == 0 {
                        break;
                    }
                    continue;
                }
            }
            overlapped.token = token;
            overlapped.dw_number_of_bytes_transferred = bytes;
            cq.push(overlapped);
            if cq.len() >= want || timeout_time.saturating_sub(now()) == 0 {
                break;
            }
        }
        let cost = Instant::now().saturating_duration_since(start_time);
        Ok((cq.len(), cq, timeout.map(|t| t.saturating_sub(cost))))
    }

    pub(crate) fn accept(
        &self,
        user_data: usize,
        fd: SOCKET,
        _address: *mut SOCKADDR,
        _address_len: *mut c_int,
    ) -> std::io::Result<()> {
        if !self.handles.contains(&(fd as HANDLE)) {
            self.add_handle(fd, fd as HANDLE)?;
        }
        let context = SOCKET_CONTEXT.get(&fd).expect("socket context not found");
        let ctx = context.value();
        unsafe {
            let socket = windows_sys::Win32::Networking::WinSock::WSASocketW(
                ctx.domain,
                ctx.ty,
                ctx.protocol,
                std::ptr::null(),
                0,
                WSA_FLAG_OVERLAPPED,
            );
            if INVALID_SOCKET == socket {
                return Err(Error::new(ErrorKind::Other, "add accept operation failed"));
            }
            let size = size_of::<SOCKADDR_IN>()
                .saturating_add(16)
                .try_into()
                .expect("size overflow");
            let mut lpdwbytesreceived = 0;
            let mut lpoverlapped: Overlapped = std::mem::zeroed();
            lpoverlapped.from_fd = fd;
            lpoverlapped.socket = socket;
            lpoverlapped.token = user_data;
            lpoverlapped.syscall = Syscall::accept;
            while windows_sys::Win32::Networking::WinSock::AcceptEx(
                fd,
                socket,
                std::ptr::null_mut(),
                0,
                size,
                size,
                &mut lpdwbytesreceived,
                (&mut lpoverlapped as *mut Overlapped).cast(),
            ) == FALSE
            {
                if WSA_IO_PENDING == WSAGetLastError() {
                    break;
                }
            }
        }
        Ok(())
    }
}
