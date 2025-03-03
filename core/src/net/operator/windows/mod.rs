use crate::common::constants::SyscallName;
use crate::common::{get_timeout_time, now};
use crate::impl_display_by_debug;
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
    getsockopt, setsockopt, AcceptEx, WSAGetLastError, WSARecv, WSASend, WSASocketW,
    INVALID_SOCKET, LPCONDITIONPROC, LPWSAOVERLAPPED_COMPLETION_ROUTINE, SEND_RECV_FLAGS, SOCKADDR,
    SOCKADDR_IN, SOCKET, SOCKET_ERROR, SOL_SOCKET, SO_PROTOCOL_INFO, SO_UPDATE_ACCEPT_CONTEXT,
    WSABUF, WSAEINPROGRESS, WSAENETDOWN, WSAPROTOCOL_INFOW, WSA_FLAG_OVERLAPPED, WSA_IO_PENDING,
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
    pub token: usize,
    syscall_name: SyscallName,
    socket: SOCKET,
    pub result: c_longlong,
}

impl Default for Overlapped {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl_display_by_debug!(Overlapped);

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
            if ret.is_null()
                && ERROR_INVALID_PARAMETER == WSAGetLastError().try_into().expect("overflow")
            {
                // duplicate bind
                return Ok(());
            }
            debug_assert_eq!(ret, self.iocp);
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
                    &mut recv_count,
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
                        SyscallName::accept => {
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
                        _ => panic!("unsupported"),
                    };
                    eprintln!("IOCP got:{cqe}");
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

    pub(crate) fn accept(
        &self,
        user_data: usize,
        fd: SOCKET,
        _address: *mut SOCKADDR,
        _address_len: *mut c_int,
    ) -> std::io::Result<()> {
        self.acceptex(user_data, fd, SyscallName::accept)
    }

    pub(crate) fn WSAAccept(
        &self,
        user_data: usize,
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
        user_data: usize,
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
                &mut sock_info_len,
            ) != 0
            {
                return Err(Error::other("get socket info failed"));
            }
            self.add_handle(fd as HANDLE)?;
            let socket = WSASocketW(
                sock_info.iAddressFamily,
                sock_info.iSocketType,
                sock_info.iProtocol,
                &sock_info,
                0,
                WSA_FLAG_OVERLAPPED,
            );
            if INVALID_SOCKET == socket {
                return Err(Error::other(format!("add {syscall_name} operation failed")));
            }
            let size = size_of::<SOCKADDR_IN>()
                .saturating_add(16)
                .try_into()
                .expect("size overflow");
            let overlapped: &'o mut Overlapped = Box::leak(Box::default());
            overlapped.from_fd = fd;
            overlapped.token = user_data;
            overlapped.syscall_name = syscall_name;
            overlapped.socket = socket;
            overlapped.result = -c_longlong::from(WSAENETDOWN);
            let mut buf: Vec<u8> = Vec::with_capacity(size as usize * 2);
            while AcceptEx(
                fd,
                socket,
                buf.as_mut_ptr().cast(),
                0,
                size,
                size,
                std::ptr::null_mut(),
                std::ptr::from_mut(overlapped).cast(),
            ) == FALSE
            {
                if WSA_IO_PENDING == WSAGetLastError() {
                    break;
                }
            }
            eprintln!("add {syscall_name} operation:{overlapped}");
        }
        Ok(())
    }

    pub(crate) fn recv(
        &self,
        user_data: usize,
        fd: SOCKET,
        buf: PSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> std::io::Result<()> {
        let buf = [WSABUF {
            len: len.try_into().expect("len overflow"),
            buf: buf.cast(),
        }];
        self.wsarecv(
            user_data,
            fd,
            buf.as_ptr(),
            buf.len().try_into().expect("len overflow"),
            std::ptr::null_mut(),
            &mut c_uint::try_from(flags).expect("overflow"),
            None,
            SyscallName::recv,
        )
    }

    pub(crate) fn WSARecv(
        &self,
        user_data: usize,
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
        user_data: usize,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags: *mut c_uint,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        syscall_name: SyscallName,
    ) -> std::io::Result<()> {
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped: &'o mut Overlapped = Box::leak(Box::default());
            overlapped.from_fd = fd;
            overlapped.token = user_data;
            overlapped.syscall_name = syscall_name;
            overlapped.result = -c_longlong::from(WSAEINPROGRESS);
            if WSARecv(
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
                    return Err(Error::other(format!(
                        "add {syscall_name} operation failed with {errno}"
                    )));
                }
            }
            eprintln!("add {syscall_name} operation:{overlapped}");
        }
        Ok(())
    }

    pub(crate) fn send(
        &self,
        user_data: usize,
        fd: SOCKET,
        buf: PCSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS,
    ) -> std::io::Result<()> {
        let buf = [WSABUF {
            len: len.try_into().expect("len overflow"),
            buf: buf.cast_mut(),
        }];
        self.wsasend(
            user_data,
            fd,
            buf.as_ptr(),
            buf.len().try_into().expect("len overflow"),
            std::ptr::null_mut(),
            c_uint::try_from(flags).expect("overflow"),
            None,
            SyscallName::send,
        )
    }

    pub(crate) fn WSASend(
        &self,
        user_data: usize,
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
        user_data: usize,
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags: c_uint,
        lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
        syscall_name: SyscallName,
    ) -> std::io::Result<()> {
        self.add_handle(fd as HANDLE)?;
        unsafe {
            let overlapped: &'o mut Overlapped = Box::leak(Box::default());
            overlapped.from_fd = fd;
            overlapped.token = user_data;
            overlapped.syscall_name = syscall_name;
            overlapped.result = -c_longlong::from(WSAEINPROGRESS);
            if WSASend(
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
                    return Err(Error::other(format!(
                        "add {syscall_name} operation failed with {errno}"
                    )));
                }
            }
            eprintln!("add {syscall_name} operation:{overlapped}");
        }
        Ok(())
    }
}
