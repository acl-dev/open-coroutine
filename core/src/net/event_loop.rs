use crate::co_pool::CoroutinePool;
use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, PoolState, Syscall, SyscallState, SLICE};
use crate::net::selector::{Event, Events, Poller, Selector};
use crate::scheduler::SchedulableCoroutine;
use crate::{error, impl_current_for, impl_display_by_debug, info};
use crossbeam_utils::atomic::AtomicCell;
use dashmap::DashSet;
#[cfg(all(target_os = "linux", feature = "io_uring"))]
use libc::{epoll_event, iovec, msghdr, off_t, size_t, sockaddr, socklen_t};
use once_cell::sync::Lazy;
use rand::Rng;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

cfg_if::cfg_if! {
    if #[cfg(all(windows, feature = "iocp"))] {
        use std::ffi::c_uint;
        use windows_sys::core::{PCSTR, PSTR};
        use windows_sys::Win32::Networking::WinSock::{
            setsockopt, LPWSAOVERLAPPED_COMPLETION_ROUTINE, SEND_RECV_FLAGS, SOCKADDR, SOCKET, SOL_SOCKET,
            SO_UPDATE_ACCEPT_CONTEXT, WSABUF,
        };
        use windows_sys::Win32::System::IO::OVERLAPPED;
    }
}

cfg_if::cfg_if! {
    if #[cfg(any(all(target_os = "linux", feature = "io_uring"), all(windows, feature = "iocp")))] {
        use dashmap::DashMap;
        use std::ffi::c_longlong;
    }
}

#[repr(C)]
#[derive(Debug)]
pub(crate) struct EventLoop<'e> {
    //状态
    state: AtomicCell<PoolState>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    shared_stop: Arc<(Mutex<AtomicUsize>, Condvar)>,
    cpu: usize,
    #[cfg(any(
        all(target_os = "linux", feature = "io_uring"),
        all(windows, feature = "iocp")
    ))]
    operator: crate::net::operator::Operator<'e>,
    #[allow(clippy::type_complexity)]
    #[cfg(any(
        all(target_os = "linux", feature = "io_uring"),
        all(windows, feature = "iocp")
    ))]
    syscall_wait_table: DashMap<usize, Arc<(Mutex<Option<c_longlong>>, Condvar)>>,
    selector: Poller,
    pool: CoroutinePool<'e>,
    phantom_data: PhantomData<&'e EventLoop<'e>>,
}

impl<'e> Deref for EventLoop<'e> {
    type Target = CoroutinePool<'e>;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl DerefMut for EventLoop<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool
    }
}

impl Default for EventLoop<'_> {
    fn default() -> Self {
        let max_cpu_index = num_cpus::get();
        let random_cpu_index = rand::thread_rng().gen_range(0..max_cpu_index);
        Self::new(
            format!("open-coroutine-event-loop-{random_cpu_index}"),
            random_cpu_index,
            crate::common::constants::DEFAULT_STACK_SIZE,
            0,
            65536,
            0,
            Arc::new((Mutex::new(AtomicUsize::new(0)), Condvar::new())),
        )
        .expect("create event-loop failed")
    }
}

static COROUTINE_TOKENS: Lazy<DashSet<usize>> = Lazy::new(DashSet::new);

impl<'e> EventLoop<'e> {
    pub(super) fn new(
        name: String,
        cpu: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        shared_stop: Arc<(Mutex<AtomicUsize>, Condvar)>,
    ) -> std::io::Result<Self> {
        Ok(EventLoop {
            state: AtomicCell::new(PoolState::Running),
            stop: Arc::new((Mutex::new(false), Condvar::new())),
            shared_stop,
            cpu,
            #[cfg(any(
                all(target_os = "linux", feature = "io_uring"),
                all(windows, feature = "iocp")
            ))]
            operator: crate::net::operator::Operator::new(cpu)?,
            #[cfg(any(
                all(target_os = "linux", feature = "io_uring"),
                all(windows, feature = "iocp")
            ))]
            syscall_wait_table: DashMap::new(),
            selector: Poller::new()?,
            pool: CoroutinePool::new(name, stack_size, min_size, max_size, keep_alive_time),
            phantom_data: PhantomData,
        })
    }

    #[allow(trivial_numeric_casts, clippy::cast_possible_truncation)]
    fn token(syscall: Syscall) -> usize {
        if let Some(co) = SchedulableCoroutine::current() {
            let boxed: &'static mut CString = Box::leak(Box::from(
                CString::new(co.name()).expect("build name failed!"),
            ));
            let cstr: &'static CStr = boxed.as_c_str();
            let token = cstr.as_ptr().cast::<c_void>() as usize;
            assert!(COROUTINE_TOKENS.insert(token));
            return token;
        }
        unsafe {
            cfg_if::cfg_if! {
                if #[cfg(windows)] {
                    let thread_id = windows_sys::Win32::System::Threading::GetCurrentThread();
                } else {
                    let thread_id = libc::pthread_self();
                }
            }
            let syscall_mask = <Syscall as Into<&str>>::into(syscall).as_ptr() as usize;
            let token = thread_id as usize ^ syscall_mask;
            if Syscall::nio() != syscall {
                eprintln!("generate token:{token} for {syscall}");
            }
            token
        }
    }

    pub(super) fn add_read_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector
            .add_read_event(fd, EventLoop::token(Syscall::nio()))
    }

    pub(super) fn add_write_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector
            .add_write_event(fd, EventLoop::token(Syscall::nio()))
    }

    pub(super) fn del_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_event(fd)
    }

    pub(super) fn del_read_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_read_event(fd)
    }

    pub(super) fn del_write_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_write_event(fd)
    }

    pub(super) fn wait_event(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        let left_time = if SchedulableCoroutine::current().is_some() {
            timeout
        } else if let Some(time) = timeout {
            Some(
                self.try_timed_schedule_task(time)
                    .map(Duration::from_nanos)?,
            )
        } else {
            self.try_schedule_task()?;
            None
        };
        self.wait_just(left_time)
    }

    /// Wait events happen.
    pub(super) fn timed_wait_just(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        let timeout_time = timeout.map_or(u64::MAX, crate::common::get_timeout_time);
        loop {
            let left_time = timeout_time
                .saturating_sub(crate::common::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return self.wait_just(Some(Duration::ZERO));
            }
            self.wait_just(Some(Duration::from_nanos(left_time)))?;
        }
    }

    pub(super) fn wait_just(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        let mut left_time = timeout;
        if let Some(time) = left_time {
            let timestamp = crate::common::get_timeout_time(time);
            if let Some(co) = SchedulableCoroutine::current() {
                if let CoroutineState::SystemCall((), syscall, SyscallState::Executing) = co.state()
                {
                    let new_state = SyscallState::Suspend(timestamp);
                    if co.syscall((), syscall, new_state).is_err() {
                        error!(
                            "{} change to syscall {} {} failed !",
                            co.name(),
                            syscall,
                            new_state
                        );
                    }
                }
            }
            if let Some(suspender) = crate::scheduler::SchedulableSuspender::current() {
                suspender.until(timestamp);
                //回来的时候等待的时间已经到了
                left_time = Some(Duration::ZERO);
            }
            if let Some(co) = SchedulableCoroutine::current() {
                if let CoroutineState::SystemCall(
                    (),
                    syscall,
                    SyscallState::Callback | SyscallState::Timeout,
                ) = co.state()
                {
                    let new_state = SyscallState::Executing;
                    if co.syscall((), syscall, new_state).is_err() {
                        error!(
                            "{} change to syscall {} {} failed !",
                            co.name(),
                            syscall,
                            new_state
                        );
                    }
                }
            }
        }

        cfg_if::cfg_if! {
            if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
                left_time = self.adapt_io_uring(left_time)?;
            } else if #[cfg(all(windows, feature = "iocp"))] {
                left_time = self.adapt_iocp(left_time)?;
            }
        }

        // use epoll/kevent/iocp
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, left_time)?;
        #[allow(clippy::explicit_iter_loop)]
        for event in events.iter() {
            let token = event.get_token();
            if event.readable() || event.writable() {
                unsafe { self.resume(token) };
            }
        }
        Ok(())
    }

    #[cfg(all(target_os = "linux", feature = "io_uring"))]
    fn adapt_io_uring(&self, mut left_time: Option<Duration>) -> std::io::Result<Option<Duration>> {
        if crate::net::operator::support_io_uring() {
            // use io_uring
            let (count, mut cq, left) = self.operator.select(left_time, 0)?;
            if count > 0 {
                for cqe in &mut cq {
                    let token = usize::try_from(cqe.user_data()).expect("token overflow");
                    if crate::common::constants::IO_URING_TIMEOUT_USERDATA == token {
                        continue;
                    }
                    // resolve completed read/write tasks
                    let result = c_longlong::from(cqe.result());
                    eprintln!("io_uring finish {token} {result}");
                    if let Some((_, pair)) = self.syscall_wait_table.remove(&token) {
                        let (lock, cvar) = &*pair;
                        let mut pending = lock.lock().expect("lock failed");
                        *pending = Some(result);
                        cvar.notify_one();
                    }
                    unsafe { self.resume(token) };
                }
            }
            if left != left_time {
                left_time = Some(left.unwrap_or(Duration::ZERO));
            }
        }
        Ok(left_time)
    }

    #[cfg(all(windows, feature = "iocp"))]
    fn adapt_iocp(&self, mut left_time: Option<Duration>) -> std::io::Result<Option<Duration>> {
        // use IOCP
        let (count, mut cq, left) = self.operator.select(left_time, 0)?;
        if count > 0 {
            for cqe in &mut cq {
                let token = cqe.token;
                let bytes_transferred = cqe.bytes_transferred;
                // resolve completed read/write tasks
                // todo refactor IOCP impl
                let result = match cqe.syscall {
                    Syscall::accept => unsafe {
                        if setsockopt(
                            cqe.socket,
                            SOL_SOCKET,
                            SO_UPDATE_ACCEPT_CONTEXT,
                            std::ptr::from_ref(&cqe.from_fd).cast(),
                            c_int::try_from(size_of::<SOCKET>()).expect("overflow"),
                        ) == 0
                        {
                            #[cfg(feature = "syscall")]
                            crate::syscall::common::reset_errno();
                            cqe.socket.try_into().expect("result overflow")
                        } else {
                            -c_longlong::from(windows_sys::Win32::Foundation::GetLastError())
                        }
                    },
                    Syscall::recv | Syscall::WSARecv | Syscall::send | Syscall::WSASend => {
                        bytes_transferred.into()
                    }
                    _ => panic!("unsupported"),
                };
                eprintln!("IOCP finish {token} {result}");
                if let Some((_, pair)) = self.syscall_wait_table.remove(&token) {
                    let (lock, cvar) = &*pair;
                    let mut pending = lock.lock().expect("lock failed");
                    *pending = Some(result);
                    cvar.notify_one();
                }
                unsafe { self.resume(token) };
            }
        }
        if left != left_time {
            left_time = Some(left.unwrap_or(Duration::ZERO));
        }
        Ok(left_time)
    }

    #[allow(clippy::unused_self)]
    unsafe fn resume(&self, token: usize) {
        if COROUTINE_TOKENS.remove(&token).is_none() {
            return;
        }
        if let Ok(co_name) = CStr::from_ptr((token as *const c_void).cast::<c_char>()).to_str() {
            self.try_resume(co_name);
        }
    }

    pub(super) fn start(self) -> std::io::Result<Arc<Self>>
    where
        'e: 'static,
    {
        // init stop flag
        {
            let (lock, cvar) = &*self.stop;
            let mut pending = lock.lock().expect("lock failed");
            *pending = true;
            cvar.notify_one();
        }
        let thread_name = self.get_thread_name();
        let bean_name = self.name().to_string().leak();
        let bean_name_in_thread = self.name().to_string().leak();
        BeanFactory::init_bean(bean_name, self);
        BeanFactory::init_bean(
            &thread_name,
            std::thread::Builder::new()
                .name(thread_name.clone())
                .spawn(move || {
                    let consumer =
                        unsafe { BeanFactory::get_mut_bean::<Self>(bean_name_in_thread) }
                            .unwrap_or_else(|| panic!("bean {bean_name_in_thread} not exist !"));
                    {
                        let (lock, cvar) = &*consumer.shared_stop.clone();
                        let started = lock.lock().expect("lock failed");
                        _ = started.fetch_add(1, Ordering::Release);
                        cvar.notify_one();
                    }
                    // thread per core
                    info!(
                        "{} has started, bind to CPU:{}",
                        consumer.name(),
                        core_affinity::set_for_current(core_affinity::CoreId { id: consumer.cpu })
                    );
                    Self::init_current(consumer);
                    while PoolState::Running == consumer.state()
                        || !consumer.is_empty()
                        || consumer.get_running_size() > 0
                    {
                        _ = consumer.wait_event(Some(SLICE));
                    }
                    // notify stop flags
                    {
                        let (lock, cvar) = &*consumer.stop.clone();
                        let mut pending = lock.lock().expect("lock failed");
                        *pending = false;
                        cvar.notify_one();
                    }
                    {
                        let (lock, cvar) = &*consumer.shared_stop.clone();
                        let started = lock.lock().expect("lock failed");
                        _ = started.fetch_sub(1, Ordering::Release);
                        cvar.notify_one();
                    }
                    Self::clean_current();
                    info!("{} has exited", consumer.name());
                })?,
        );
        unsafe {
            Ok(Arc::from_raw(
                BeanFactory::get_bean::<Self>(bean_name)
                    .unwrap_or_else(|| panic!("bean {bean_name} not exist !")),
            ))
        }
    }

    fn get_thread_name(&self) -> String {
        format!("{}-thread", self.name())
    }

    pub(super) fn stop_sync(&mut self, wait_time: Duration) -> std::io::Result<()> {
        match self.state() {
            PoolState::Running => {
                assert_eq!(PoolState::Running, self.stopping()?);
                let timeout_time = crate::common::get_timeout_time(wait_time);
                loop {
                    let left_time = timeout_time.saturating_sub(crate::common::now());
                    if 0 == left_time {
                        return Err(Error::new(ErrorKind::TimedOut, "stop timeout !"));
                    }
                    self.wait_event(Some(Duration::from_nanos(left_time).min(SLICE)))?;
                    if self.is_empty() && self.get_running_size() == 0 {
                        assert_eq!(PoolState::Stopping, self.stopped()?);
                        return Ok(());
                    }
                }
            }
            PoolState::Stopping => Err(Error::new(ErrorKind::Other, "should never happens")),
            PoolState::Stopped => Ok(()),
        }
    }

    pub(super) fn stop(&self, wait_time: Duration) -> std::io::Result<()> {
        match self.state() {
            PoolState::Running => {
                if BeanFactory::remove_bean::<JoinHandle<()>>(&self.get_thread_name()).is_some() {
                    assert_eq!(PoolState::Running, self.stopping()?);
                    //开启了单独的线程
                    let (lock, cvar) = &*self.stop;
                    let result = cvar
                        .wait_timeout_while(
                            lock.lock().expect("lock failed"),
                            wait_time,
                            |&mut pending| pending,
                        )
                        .expect("lock failed");
                    if result.1.timed_out() {
                        return Err(Error::new(ErrorKind::TimedOut, "stop timeout !"));
                    }
                    assert_eq!(PoolState::Stopping, self.stopped()?);
                }
                Ok(())
            }
            PoolState::Stopping => Err(Error::new(ErrorKind::Other, "should never happens")),
            PoolState::Stopped => Ok(()),
        }
    }
}

impl_current_for!(EVENT_LOOP, EventLoop<'e>);

impl_display_by_debug!(EventLoop<'e>);

macro_rules! impl_io_uring {
    ( $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        impl EventLoop<'_> {
            pub(super) fn $syscall(
                &self,
                $($arg: $arg_type),*
            ) -> std::io::Result<Arc<(Mutex<Option<c_longlong>>, Condvar)>> {
                let token = EventLoop::token(Syscall::$syscall);
                self.operator.$syscall(token, $($arg, )*)?;
                let arc = Arc::new((Mutex::new(None), Condvar::new()));
                assert!(
                    self.syscall_wait_table.insert(token, arc.clone()).is_none(),
                    "The previous token was not retrieved in a timely manner"
                );
                Ok(arc)
            }
        }
    }
}

impl_io_uring!(epoll_ctl(epfd: c_int, op: c_int, fd: c_int, event: *mut epoll_event) -> c_int);
impl_io_uring!(socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int);
impl_io_uring!(accept(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int);
impl_io_uring!(accept4(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t, flg: c_int) -> c_int);
impl_io_uring!(shutdown(fd: c_int, how: c_int) -> c_int);
impl_io_uring!(connect(fd: c_int, address: *const sockaddr, len: socklen_t) -> c_int);
impl_io_uring!(close(fd: c_int) -> c_int);
impl_io_uring!(recv(fd: c_int, buf: *mut c_void, len: size_t, flags: c_int) -> ssize_t);
impl_io_uring!(read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t);
impl_io_uring!(pread(fd: c_int, buf: *mut c_void, count: size_t, offset: off_t) -> ssize_t);
impl_io_uring!(readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_io_uring!(preadv(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_io_uring!(recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> ssize_t);
impl_io_uring!(send(fd: c_int, buf: *const c_void, len: size_t, flags: c_int) -> ssize_t);
impl_io_uring!(sendto(fd: c_int, buf: *const c_void, len: size_t, flags: c_int, addr: *const sockaddr, addrlen: socklen_t) -> ssize_t);
impl_io_uring!(write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t);
impl_io_uring!(pwrite(fd: c_int, buf: *const c_void, count: size_t, offset: off_t) -> ssize_t);
impl_io_uring!(writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> ssize_t);
impl_io_uring!(pwritev(fd: c_int, iov: *const iovec, iovcnt: c_int, offset: off_t) -> ssize_t);
impl_io_uring!(sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> ssize_t);

macro_rules! impl_iocp {
    ( $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[cfg(all(windows, feature = "iocp"))]
        impl EventLoop<'_> {
            #[allow(non_snake_case, clippy::too_many_arguments)]
            pub(super) fn $syscall(
                &self,
                $($arg: $arg_type),*
            ) -> std::io::Result<Arc<(Mutex<Option<c_longlong>>, Condvar)>> {
                let token = EventLoop::token(Syscall::$syscall);
                self.operator.$syscall(token, $($arg, )*)?;
                let arc = Arc::new((Mutex::new(None), Condvar::new()));
                assert!(
                    self.syscall_wait_table.insert(token, arc.clone()).is_none(),
                    "The previous token was not retrieved in a timely manner"
                );
                Ok(arc)
            }
        }
    }
}

impl_iocp!(accept(fd: SOCKET, addr: *mut SOCKADDR, len: *mut c_int) -> c_int);
impl_iocp!(recv(fd: SOCKET, buf: PSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int);
impl_iocp!(WSARecv(fd: SOCKET, buf: *const WSABUF, dwbuffercount: c_uint, lpnumberofbytesrecvd: *mut c_uint, lpflags : *mut c_uint, lpoverlapped: *mut OVERLAPPED, lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE) -> c_int);
impl_iocp!(send(fd: SOCKET, buf: PCSTR, len: c_int, flags: SEND_RECV_FLAGS) -> c_int);
impl_iocp!(WSASend(fd: SOCKET, buf: *const WSABUF, dwbuffercount: c_uint, lpnumberofbytesrecvd: *mut c_uint, dwflags : c_uint, lpoverlapped: *mut OVERLAPPED, lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE) -> c_int);

#[cfg(all(test, not(all(unix, feature = "preemptive"))))]
mod tests {
    use crate::net::event_loop::EventLoop;
    use std::time::Duration;

    #[test]
    fn test_simple() -> std::io::Result<()> {
        let mut event_loop = EventLoop::default();
        event_loop.set_max_size(1);
        _ = event_loop.submit_task(None, |_| panic!("test panic, just ignore it"), None)?;
        _ = event_loop.submit_task(
            None,
            |_| {
                println!("2");
                Some(2)
            },
            None,
        )?;
        event_loop.stop_sync(Duration::from_secs(3))
    }

    #[ignore]
    #[test]
    fn test_simple_auto() -> std::io::Result<()> {
        let event_loop = EventLoop::default().start()?;
        event_loop.set_max_size(1);
        _ = event_loop.submit_task(None, |_| panic!("test panic, just ignore it"), None)?;
        _ = event_loop.submit_task(
            None,
            |_| {
                println!("2");
                Some(2)
            },
            None,
        )?;
        event_loop.stop(Duration::from_secs(3))
    }
}
