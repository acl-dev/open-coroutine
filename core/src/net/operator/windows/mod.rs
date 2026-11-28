use crate::common::constants::SyscallName;
use crate::common::{get_timeout_time, now};
use crate::impl_display_by_debug;
use dashmap::DashSet;
use once_cell::sync::Lazy;
use std::ffi::{c_int, c_longlong, c_uint};
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows_sys::core::{PCSTR, PSTR};
use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, FALSE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Networking::WinSock::{
    getsockopt, setsockopt, AcceptEx, WSAGetLastError, WSARecvFrom, WSASendTo, WSASocketW,
    INVALID_SOCKET, LPCONDITIONPROC, LPWSAOVERLAPPED_COMPLETION_ROUTINE, SEND_RECV_FLAGS, SOCKADDR,
    SOCKADDR_IN6, SOCKET, SOCKET_ERROR, SOL_SOCKET, SO_PROTOCOL_INFO, SO_UPDATE_ACCEPT_CONTEXT,
    WSABUF, WSAEINPROGRESS, WSAEINVAL, WSAENETDOWN, WSAPROTOCOL_INFOW, WSA_FLAG_OVERLAPPED,
    WSA_IO_PENDING,
};
use windows_sys::Win32::Storage::FileSystem::SetFileCompletionNotificationModes;
use windows_sys::Win32::System::WindowsProgramming::FILE_SKIP_SET_EVENT_ON_HANDLE;
use windows_sys::Win32::System::IO::{
    CreateIoCompletionPort, GetQueuedCompletionStatusEx, OVERLAPPED, OVERLAPPED_ENTRY,
};

#[cfg(test)]
mod tests;

/// The overlapped struct we actually used for IOCP.
#[repr(C)]
#[derive(educe::Educe)]
#[educe(Debug)]
pub(crate) struct Overlapped {
    /// The base [`OVERLAPPED`].
    #[educe(Debug(ignore))]
    base: OVERLAPPED,
    from_fd: SOCKET,
    pub token: u64,
    syscall_name: SyscallName,
    socket: SOCKET,
    pub result: c_longlong,
    /// Owned `WSABUF` storage for single-buffer `recv`/`send` operations.
    /// Must stay alive until the IOCP completion is dequeued.
    #[educe(Debug(ignore))]
    wsabuf: WSABUF,
    /// Owned `lpflags` storage for `recv`/`send` operations.
    #[educe(Debug(ignore))]
    flags: c_uint,
    /// Owned buffer for `AcceptEx` local/remote address output.
    /// Sized for IPv6 (`2 * (sizeof(SOCKADDR_IN6) + 16)`).
    #[educe(Debug(ignore))]
    accept_buf: [u8; 128],
}

impl Default for Overlapped {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl_display_by_debug!(Overlapped);

/// Sockets that support overlapped I/O: created through `socket`/`WSASocketW`
/// with `WSA_FLAG_OVERLAPPED`, or accepted via `AcceptEx`. Overlapped
/// operations submitted for any other socket are rejected so the caller can
/// fall back to the NIO path.
pub(crate) static OVERLAPPED_SOCKETS: Lazy<DashSet<SOCKET>> = Lazy::new(DashSet::new);

/// Sockets already associated with an open-coroutine IOCP port. A socket can
/// only be bound to one completion port; this distinguishes "already bound to
/// us" (duplicate bind, safe to proceed) from "bound elsewhere" such as the
/// mio selector's port (completions would be delivered where we never look).
static BOUND_SOCKETS: Lazy<DashSet<SOCKET>> = Lazy::new(DashSet::new);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Operator<'o> {
    cpu: usize,
    iocp: HANDLE,
    entering: AtomicBool,
    phantom_data: PhantomData<&'o Overlapped>,
}

impl<'o> Operator<'o> {
    pub(crate) fn new(cpu: usize) -> std::io::Result<Self> {
        let iocp =
            unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 0) };
        if iocp.is_null() {
            return Err(Error::last_os_error());
        }
        Ok(Self {
            cpu,
            iocp,
            entering: AtomicBool::new(false),
            phantom_data: PhantomData,
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
    fn add_handle(&self, handle: HANDLE) -> std::io::Result<()> {
        unsafe {
            let ret = CreateIoCompletionPort(handle, self.iocp, self.cpu, 0);
            if ret.is_null() {
                let err = WSAGetLastError();
                if ERROR_INVALID_PARAMETER == TryInto::<u32>::try_into(err).expect("overflow") {
                    // duplicate bind: only safe when the socket is already
                    // bound to one of our IOCP ports.
                    if BOUND_SOCKETS.contains(&(handle as SOCKET)) {
                        return Ok(());
                    }
                    return Err(Error::new(
                        ErrorKind::WouldBlock,
                        "socket is bound to another completion port",
                    ));
                }
                return Err(Error::last_os_error());
            }
            debug_assert_eq!(ret, self.iocp);
            _ = BOUND_SOCKETS.insert(handle as SOCKET);
            if SetFileCompletionNotificationModes(
                handle,
                u8::try_from(FILE_SKIP_SET_EVENT_ON_HANDLE).expect("overflow"),
            ) == 0
            {
                return Err(Error::last_os_error());
            }
        }
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
        result
    }

    #[allow(clippy::cast_ptr_alignment)]
    fn do_select(
        &self,
        timeout: Option<Duration>,
        want: usize,
    ) -> std::io::Result<(usize, Vec<Overlapped>, Option<Duration>)> {
        let start_time = Instant::now();
        let timeout_time = timeout.map_or(u64::MAX, get_timeout_time);
        let mut cq = Vec::new();
        loop {
            let left_ns = timeout_time.saturating_sub(now());
            if left_ns == 0 {
                break;
            }
            let mut entries: Vec<OVERLAPPED_ENTRY> = Vec::with_capacity(1024);
            let uninit = entries.spare_capacity_mut();
            let mut recv_count = 0;
            unsafe {
                let ret = GetQueuedCompletionStatusEx(
                    self.iocp,
                    uninit.as_mut_ptr().cast(),
                    uninit.len().try_into().expect("overflow"),
                    &raw mut recv_count,
                    left_ns
                        .saturating_div(1_000_000)
                        .try_into()
                        .unwrap_or(u32::MAX),
                    0,
                );
                if FALSE == ret {
                    let e = Error::last_os_error();
                    if ErrorKind::TimedOut == e.kind() {
                        continue;
                    }
                    return Err(e);
                }
                entries.set_len(recv_count as _);
                for entry in entries {
                    let mut cqe = Box::from_raw(entry.lpOverlapped.cast::<Overlapped>());
                    // resolve completed read/write tasks
                    cqe.result = match cqe.syscall_name {
                        SyscallName::accept | SyscallName::WSAAccept => {
                            if setsockopt(
                                cqe.socket,
                                SOL_SOCKET,
                                SO_UPDATE_ACCEPT_CONTEXT,
                                std::ptr::from_ref(&cqe.from_fd).cast(),
                                c_int::try_from(size_of::<SOCKET>()).expect("overflow"),
                            ) == 0
                            {
                                cqe.socket.try_into().expect("result overflow")
                            } else {
                                -c_longlong::from(WSAENETDOWN)
                            }
                        }
                        SyscallName::recv
                        | SyscallName::WSARecv
                        | SyscallName::send
                        | SyscallName::WSASend => entry.dwNumberOfBytesTransferred.into(),
                        // `error!` expands to nothing without the `log` feature,
                        // leaving `other` unused in that configuration.
                        #[allow(unused_variables)]
                        other => {
                            crate::error!("unsupported IOCP syscall {} completed", other);
                            -c_longlong::from(WSAEINVAL)
                        }
                    };
                    cq.push(*cqe);
                }
            }
            if cq.len() >= want {
                break;
            }
        }
        let cost = Instant::now().saturating_duration_since(start_time);
        Ok((cq.len(), cq, timeout.map(|t| t.saturating_sub(cost))))
    }

    pub(crate) fn async_cancel(_user_data: u64) -> std::io::Result<()> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "IOCP async_cancel is not implemented",
        ))
    }

    pub(crate) fn accept(
        &self,
        user_data: u64,
        fd: SOCKET,
        _address: *mut SOCKADDR,
        _address_len: *mut c_int,
    ) -> std::io::Result<()> {
        self.acceptex(user_data, fd, SyscallName::accept)
    }

    pub(crate) fn WSAAccept(
        &self,
        user_data: u64,
        fd: SOCKET,
        _address: *mut SOCKADDR,
        _address_len: *mut c_int,
        lpfncondition: LPCONDITIONPROC,
        _dwcallbackdata: usize,
    ) -> std::io::Result<()> {
        if lpfncondition.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "the WSAAccept in Operator should be called without lpfncondition!",
            ));
        }
        self.acceptex(user_data, fd, SyscallName::WSAAccept)
    }

    fn acceptex(
        &self,
        user_data: u64,
        fd: SOCKET,
        syscall_name: SyscallName,
    ) -> std::io::Result<()> {
        unsafe {
            let mut sock_info: WSAPROTOCOL_INFOW = std::mem::zeroed();
            let mut sock_info_len = size_of::<WSAPROTOCOL_INFOW>()
                .try_into()
                .expect("sock_info_len overflow");
            if getsockopt(
                fd,
                SOL_SOCKET,
                SO_PROTOCOL_INFO,
                std::ptr::from_mut(&mut sock_info).cast(),
                &raw mut sock_info_len,
            ) != 0
            {
                return Err(Error::other("get socket info failed"));
            }
            self.add_handle(fd as HANDLE)?;
            let socket = WSASocketW(
                sock_info.iAddressFamily,
                sock_info.iSocketType,
                sock_info.iProtocol,
                &raw const sock_info,
                0,
                WSA_FLAG_OVERLAPPED,
            );
            if INVALID_SOCKET == socket {
                return Err(Error::other(format!("add {syscall_name} operation failed")));
            }
            // AcceptEx requires each address buffer to hold at least
            // sizeof(SOCKADDR_IN6) + 16 bytes; the output buffer must fit both
            // the local and remote addresses.
            let addr_len: u32 = (size_of::<SOCKADDR_IN6>() + 16)
                .try_into()
                .expect("addr_len overflow");
            let overlapped = Self::new_overlapped(user_data, fd, syscall_name);
            overlapped.socket = socket;
            overlapped.result = -c_longlong::from(WSAENETDOWN);
            _ = OVERLAPPED_SOCKETS.insert(socket);
            let ret = AcceptEx(
                fd,
                socket,
                overlapped.accept_buf.as_mut_ptr().cast(),
                0,
                addr_len,
                addr_len,
                std::ptr::null_mut(),
                std::ptr::from_mut(overlapped).cast(),
            );
            if ret == FALSE {
                let errno = WSAGetLastError();
                if WSA_IO_PENDING != errno {
                    // Reclaim the leaked overlapped before reporting the error.
                    Self::reclaim(std::ptr::from_mut(overlapped));
                    return Err(Error::other(format!(
                        "add {syscall_name} operation failed with {errno}"
                    )));
                }
            }
            // `ret != FALSE` means the accept completed synchronously; a
            // completion packet is still queued to the IOCP either way.
        }
        Ok(())
    }

    fn new_overlapped(user_data: u64, fd: SOCKET, syscall_name: SyscallName) -> &'o mut Overlapped {
        let overlapped: &'o mut Overlapped = Box::leak(Box::default());
        overlapped.from_fd = fd;
        overlapped.token = user_data;
        overlapped.syscall_name = syscall_name;
        overlapped.result = -c_longlong::from(WSAEINPROGRESS);
        overlapped
    }

    /// Reclaim an [`Overlapped`] that was leaked by [`Self::new_overlapped`]
    /// but whose operation failed before being queued to the IOCP.
    unsafe fn reclaim(overlapped: *mut Overlapped) {
        drop(Box::from_raw(overlapped));
    }

    /// `WSARecv`/`WSASend` are hooked by `open-coroutine-hook`, so calling
    /// them here would re-enter the coroutine syscall layer. The `*From`/`*To`
    /// variants are not hooked and are signature-compatible when the address
    /// arguments are null, so they reach the real overlapped submission.
    fn recv_from(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int {
        unsafe {
            WSARecvFrom(
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytesrecvd,
                lpflags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                lpoverlapped,
                lpcompletionroutine,
            )
        }
    }

    fn send_to(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytessent: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> c_int {
        unsafe {
            WSASendTo(
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytessent,
                dwflags,
                std::ptr::null(),
                0,
                lpoverlapped,
                lpcompletionroutine,
            )
        }
    }

    pub(crate) fn recv(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: PSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> std::io::Result<()> {
        if !OVERLAPPED_SOCKETS.contains(&fd) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "socket does not support overlapped I/O",
            ));
        }
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped = Self::new_overlapped(user_data, fd, SyscallName::recv);
            overlapped.wsabuf = WSABUF {
                len: len.try_into().expect("len overflow"),
                buf: buf.cast(),
            };
            overlapped.flags = c_uint::try_from(flags).expect("overflow");
            // The WSABUF and lpflags live inside `overlapped`, which stays
            // alive until the IOCP completion is dequeued.
            if Self::recv_from(
                fd,
                std::ptr::from_ref(&overlapped.wsabuf),
                1,
                std::ptr::null_mut(),
                std::ptr::from_mut(&mut overlapped.flags),
                std::ptr::from_mut(overlapped).cast(),
                None,
            ) == SOCKET_ERROR
            {
                let errno = WSAGetLastError();
                if WSA_IO_PENDING != errno {
                    Self::reclaim(std::ptr::from_mut(overlapped));
                    return Err(Error::other(format!(
                        "add recv operation failed with {errno}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn WSARecv(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> std::io::Result<()> {
        if !lpoverlapped.is_null() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "the WSARecv in Operator should be called without lpoverlapped!",
            ));
        }
        self.wsarecv(
            user_data,
            fd,
            buf,
            dwbuffercount,
            lpnumberofbytesrecvd,
            lpflags,
            lpcompletionroutine,
            SyscallName::WSARecv,
        )
    }

    fn wsarecv(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        syscall_name: SyscallName,
    ) -> std::io::Result<()> {
        if !OVERLAPPED_SOCKETS.contains(&fd) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "socket does not support overlapped I/O",
            ));
        }
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped = Self::new_overlapped(user_data, fd, syscall_name);
            // `buf`, `lpnumberofbytesrecvd` and `lpflags` are caller-owned and
            // must outlive the operation, matching the WSARecv contract.
            if Self::recv_from(
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytesrecvd,
                lpflags,
                std::ptr::from_mut(overlapped).cast(),
                lpcompletionroutine,
            ) == SOCKET_ERROR
            {
                let errno = WSAGetLastError();
                if WSA_IO_PENDING != errno {
                    Self::reclaim(std::ptr::from_mut(overlapped));
                    return Err(Error::other(format!(
                        "add {syscall_name} operation failed with {errno}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn send(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: PCSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> std::io::Result<()> {
        if !OVERLAPPED_SOCKETS.contains(&fd) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "socket does not support overlapped I/O",
            ));
        }
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped = Self::new_overlapped(user_data, fd, SyscallName::send);
            overlapped.wsabuf = WSABUF {
                len: len.try_into().expect("len overflow"),
                buf: buf.cast_mut(),
            };
            overlapped.flags = c_uint::try_from(flags).expect("overflow");
            if Self::send_to(
                fd,
                std::ptr::from_ref(&overlapped.wsabuf),
                1,
                std::ptr::null_mut(),
                overlapped.flags,
                std::ptr::from_mut(overlapped).cast(),
                None,
            ) == SOCKET_ERROR
            {
                let errno = WSAGetLastError();
                if WSA_IO_PENDING != errno {
                    Self::reclaim(std::ptr::from_mut(overlapped));
                    return Err(Error::other(format!(
                        "add send operation failed with {errno}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn WSASend(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags: c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
    ) -> std::io::Result<()> {
        if !lpoverlapped.is_null() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "the WSASend in Operator should be called without lpoverlapped!",
            ));
        }
        self.wsasend(
            user_data,
            fd,
            buf,
            dwbuffercount,
            lpnumberofbytesrecvd,
            dwflags,
            lpcompletionroutine,
            SyscallName::WSASend,
        )
    }

    fn wsasend(
        &self,
        user_data: u64,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags: c_uint,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        syscall_name: SyscallName,
    ) -> std::io::Result<()> {
        if !OVERLAPPED_SOCKETS.contains(&fd) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "socket does not support overlapped I/O",
            ));
        }
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped = Self::new_overlapped(user_data, fd, syscall_name);
            if Self::send_to(
                fd,
                buf,
                dwbuffercount,
                lpnumberofbytesrecvd,
                dwflags,
                std::ptr::from_mut(overlapped).cast(),
                lpcompletionroutine,
            ) == SOCKET_ERROR
            {
                let errno = WSAGetLastError();
                if WSA_IO_PENDING != errno {
                    Self::reclaim(std::ptr::from_mut(overlapped));
                    return Err(Error::other(format!(
                        "add {syscall_name} operation failed with {errno}"
                    )));
                }
            }
        }
        Ok(())
    }
}
