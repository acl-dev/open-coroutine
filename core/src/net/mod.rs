use crate::coroutine::suspender::Suspender;
use crate::net::config::Config;
use crate::net::event_loop::EventLoop;
use crate::net::join::JoinHandle;
use crate::{error, info};
use once_cell::sync::OnceCell;
use std::collections::VecDeque;
use std::ffi::c_int;
use std::io::{Error, ErrorKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
        use libc::{epoll_event, iovec, msghdr, off_t, size_t, sockaddr, socklen_t};
        use std::ffi::{c_longlong, c_void};
    }
}

/// 做C兼容时会用到
pub type UserFunc = extern "C" fn(usize) -> usize;

mod selector;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_arguments
)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
mod operator;

#[allow(missing_docs)]
pub mod event_loop;

/// Configuration for `EventLoops`.
#[allow(missing_docs)]
pub mod config;

/// Task join abstraction and impl.
pub mod join;

static INSTANCE: OnceCell<EventLoops> = OnceCell::new();

/// The manager for `EventLoop`.
#[repr(C)]
#[derive(Debug)]
pub struct EventLoops {
    index: AtomicUsize,
    loops: VecDeque<Arc<EventLoop<'static>>>,
    shared_stop: Arc<(Mutex<AtomicUsize>, Condvar)>,
}

unsafe impl Send for EventLoops {}

unsafe impl Sync for EventLoops {}

impl EventLoops {
    /// Init the `EventLoops`.
    pub fn init(config: &Config) {
        _ = INSTANCE.get_or_init(|| {
            #[cfg(feature = "ci")]
            crate::common::ci::init();
            let loops = Self::new(
                config.event_loop_size(),
                config.stack_size(),
                config.min_size(),
                config.max_size(),
                config.keep_alive_time(),
            )
            .expect("init default EventLoops failed !");
            #[cfg(feature = "log")]
            let _ = tracing_subscriber::fmt()
                .with_thread_names(true)
                .with_line_number(true)
                .with_timer(tracing_subscriber::fmt::time::OffsetTime::new(
                    time::UtcOffset::from_hms(8, 0, 0).expect("create UtcOffset failed !"),
                    time::format_description::well_known::Rfc2822,
                ))
                .try_init();
            info!("open-coroutine init with {config:#?}");
            loops
        });
    }

    /// Create a new `EventLoops`.
    pub fn new(
        event_loop_size: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
    ) -> std::io::Result<Self> {
        let shared_stop = Arc::new((Mutex::new(AtomicUsize::new(0)), Condvar::new()));
        let mut loops = VecDeque::new();
        for i in 0..event_loop_size {
            loops.push_back(
                EventLoop::new(
                    format!("open-coroutine-event-loop-{i}"),
                    i,
                    stack_size,
                    min_size,
                    max_size,
                    keep_alive_time,
                    shared_stop.clone(),
                )?
                .start()?,
            );
        }
        Ok(Self {
            index: AtomicUsize::new(0),
            loops,
            shared_stop,
        })
    }

    fn round_robin() -> &'static Arc<EventLoop<'static>> {
        let instance = INSTANCE.get().expect("EventLoops not init !");
        let index = instance.index.fetch_add(1, Ordering::Release) % instance.loops.len();
        instance
            .loops
            .get(index)
            .unwrap_or_else(move || panic!("init event-loop-{index} failed!"))
    }

    /// Get a `EventLoop`, prefer current.
    fn event_loop() -> &'static EventLoop<'static> {
        EventLoop::current().unwrap_or_else(|| Self::round_robin())
    }

    /// Submit a new task to event-loop.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    pub fn submit_task(
        name: Option<String>,
        func: impl FnOnce(Option<usize>) -> Option<usize> + 'static,
        param: Option<usize>,
    ) -> JoinHandle {
        let event_loop = Self::round_robin();
        event_loop.submit_task(name, func, param).map_or_else(
            |_| JoinHandle::err(event_loop),
            |n| JoinHandle::new(event_loop, n.as_str()),
        )
    }

    /// Submit a new coroutine to event-loop.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the pool,
    /// but only allow one thread to execute scheduling.
    pub fn submit_co(
        f: impl FnOnce(&Suspender<(), ()>, ()) -> Option<usize> + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<()> {
        Self::round_robin().submit_co(f, stack_size)
    }

    /// Waiting for read or write events to occur.
    /// This method can only be used in coroutines.
    pub fn wait_event(timeout: Option<Duration>) -> std::io::Result<()> {
        Self::event_loop().timed_wait_just(timeout)
    }

    /// Waiting for a read event to occur.
    /// This method can only be used in coroutines.
    pub fn wait_read_event(fd: c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = Self::event_loop();
        event_loop.add_read_event(fd)?;
        event_loop.wait_just(timeout)
    }

    /// Waiting for a write event to occur.
    /// This method can only be used in coroutines.
    pub fn wait_write_event(fd: c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = Self::event_loop();
        event_loop.add_write_event(fd)?;
        event_loop.wait_just(timeout)
    }

    /// Remove read and write event interests.
    /// This method can only be used in coroutines.
    pub fn del_event(fd: c_int) -> std::io::Result<()> {
        if let Some(event_loop) = EventLoop::current() {
            event_loop.del_event(fd)?;
        } else {
            let instance = INSTANCE.get().expect("EventLoops not init !");
            for event_loop in &instance.loops {
                event_loop.del_event(fd)?;
            }
        }
        Ok(())
    }

    /// Remove read event interest.
    /// This method can only be used in coroutines.
    pub fn del_read_event(fd: c_int) -> std::io::Result<()> {
        if let Some(event_loop) = EventLoop::current() {
            event_loop.del_read_event(fd)?;
        } else {
            let instance = INSTANCE.get().expect("EventLoops not init !");
            for event_loop in &instance.loops {
                event_loop.del_read_event(fd)?;
            }
        }
        Ok(())
    }

    /// Remove write event interest.
    /// This method can only be used in coroutines.
    pub fn del_write_event(fd: c_int) -> std::io::Result<()> {
        if let Some(event_loop) = EventLoop::current() {
            event_loop.del_write_event(fd)?;
        } else {
            let instance = INSTANCE.get().expect("EventLoops not init !");
            for event_loop in &instance.loops {
                event_loop.del_write_event(fd)?;
            }
        }
        Ok(())
    }

    /// Stop all `EventLoop`.
    pub fn stop(wait_time: Duration) -> std::io::Result<()> {
        if let Some(instance) = INSTANCE.get() {
            for i in &instance.loops {
                _ = i.stop(Duration::ZERO);
            }
            let (lock, cvar) = &*instance.shared_stop;
            let guard = lock
                .lock()
                .map_err(|_| Error::new(ErrorKind::TimedOut, "wait failed !"))?;
            let result = cvar
                .wait_timeout_while(guard, wait_time, |stopped| {
                    stopped.load(Ordering::Acquire) > 0
                })
                .map_err(|_| Error::new(ErrorKind::TimedOut, "wait failed !"))?;
            if result.1.timed_out() {
                error!("open-coroutine stop timeout !");
                return Err(Error::new(ErrorKind::TimedOut, "stop timeout !"));
            }
            #[cfg(all(unix, feature = "preemptive"))]
            crate::monitor::Monitor::stop();
        }
        Ok(())
    }
}

macro_rules! impl_io_uring {
    ( $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        impl EventLoops {
            #[allow(missing_docs)]
            pub fn $syscall(
                $($arg: $arg_type),*
            ) -> std::io::Result<Arc<(Mutex<Option<c_longlong>>, Condvar)>> {
                Self::event_loop().$syscall($($arg, )*)
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
