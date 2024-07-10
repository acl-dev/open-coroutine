use crate::common::Current;
use crate::coroutine::suspender::Suspender;
use crate::net::config::Config;
use crate::net::event_loop::core::EventLoop;
use crate::net::event_loop::join::{CoJoinHandle, TaskJoinHandle};
use crate::pool::task::Task;
use crate::warn;
use core_affinity::{set_for_current, CoreId};
use once_cell::sync::{Lazy, OnceCell};
use std::ffi::c_int;
use std::fmt::Debug;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
        use crate::common::Named;
        use crate::constants::{CoroutineState, SyscallState};
        use crate::scheduler::{SchedulableSuspender, SchedulableCoroutine};
        use libc::{c_void, epoll_event, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};

        macro_rules! wrap_io_uring {
            ( $syscall: ident, $($arg: expr),* $(,)* ) => {
                EventLoops::next(false)
                    .$syscall($($arg, )*)
                    .map(|r| {
                        if let Some(co) = SchedulableCoroutine::current() {
                            if let CoroutineState::SystemCall((), syscall, SyscallState::Executing) = co.state()
                            {
                                let new_state = SyscallState::Suspend(u64::MAX);
                                if co.syscall((), syscall, new_state).is_err() {
                                    crate::error!(
                                        "{} change to syscall {} {} failed !",
                                        co.get_name(),
                                        syscall,
                                        new_state
                                    );
                                }
                            }
                        }
                        if let Some(suspender) = SchedulableSuspender::current() {
                            suspender.suspend();
                            //回来的时候，系统调用已经执行完了
                        }
                        if let Some(co) = SchedulableCoroutine::current() {
                            if let CoroutineState::SystemCall((), syscall, SyscallState::Callback) = co.state()
                            {
                                let new_state = SyscallState::Executing;
                                if co.syscall((), syscall, new_state).is_err() {
                                    crate::error!(
                                        "{} change to syscall {} {} failed !",
                                        co.get_name(),
                                        syscall,
                                        new_state
                                    );
                                }
                            }
                        }
                        let (lock, cvar) = &*r;
                        let syscall_result = cvar
                            .wait_while(lock.lock().unwrap(), |&mut pending| pending.is_none())
                            .unwrap()
                            .unwrap();
                        syscall_result as _
                    })
            };
        }
    }
}

pub mod join;

mod blocker;

pub mod core;

/// 做C兼容时会用到
pub type UserFunc = extern "C" fn(*const Suspender<(), ()>, usize) -> usize;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct EventLoops {}

static INDEX: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

static mut EVENT_LOOPS: Lazy<Box<[EventLoop]>> = Lazy::new(|| {
    let config = Config::get_instance();
    warn!("open-coroutine inited with {config:#?}");
    (0..config.get_event_loop_size())
        .map(|i| {
            EventLoop::new(
                i as u32,
                config.get_stack_size(),
                config.get_min_size(),
                config.get_max_size(),
                config.get_keep_alive_time(),
            )
            .unwrap_or_else(|_| panic!("init event-loop-{i} failed!"))
        })
        .collect()
});

static EVENT_LOOP_WORKERS: OnceCell<Box<[std::thread::JoinHandle<()>]>> = OnceCell::new();

static EVENT_LOOP_STARTED: Lazy<AtomicBool> = Lazy::new(AtomicBool::default);

static EVENT_LOOP_START_COUNT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

static EVENT_LOOP_STOP: Lazy<Arc<(Mutex<AtomicUsize>, Condvar)>> =
    Lazy::new(|| Arc::new((Mutex::new(AtomicUsize::new(0)), Condvar::new())));

impl EventLoops {
    fn next(skip_monitor: bool) -> &'static mut EventLoop {
        unsafe {
            let mut index = INDEX.fetch_add(1, Ordering::SeqCst);
            if skip_monitor && index % EVENT_LOOPS.len() == 0 {
                INDEX.store(1, Ordering::SeqCst);
                EVENT_LOOPS.get_mut(1).expect("init event-loop-1 failed!")
            } else {
                index %= EVENT_LOOPS.len();
                EVENT_LOOPS
                    .get_mut(index)
                    .unwrap_or_else(|| panic!("init event-loop-{index} failed!"))
            }
        }
    }

    pub(crate) fn monitor() -> &'static EventLoop {
        //monitor线程的EventLoop固定
        unsafe {
            EVENT_LOOPS
                .first()
                .expect("init event-loop-monitor failed!")
        }
    }

    pub(crate) fn new_condition() -> Arc<(Mutex<AtomicUsize>, Condvar)> {
        Arc::clone(&EVENT_LOOP_STOP)
    }

    fn start() {
        if EVENT_LOOP_STARTED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            //初始化event_loop线程
            _ = EVENT_LOOP_WORKERS.get_or_init(|| {
                (1..unsafe { EVENT_LOOPS.len() })
                    .map(|i| {
                        std::thread::Builder::new()
                            .name(format!("open-coroutine-event-loop-{i}"))
                            .spawn(move || {
                                warn!("open-coroutine-event-loop-{i} has started");
                                _ = EVENT_LOOP_START_COUNT.fetch_add(1, Ordering::Release);
                                if set_for_current(CoreId { id: i }) {
                                    warn!("pin event-loop-{i} thread to CPU core-{i} failed !");
                                }
                                let event_loop = Self::next(true);
                                EventLoop::init_current(event_loop);
                                while EVENT_LOOP_STARTED.load(Ordering::Acquire)
                                    || event_loop.has_task()
                                {
                                    _ = event_loop.wait_event(Some(Duration::from_millis(10)));
                                }
                                EventLoop::clean_current();
                                let pair = Self::new_condition();
                                let (lock, cvar) = pair.as_ref();
                                let pending = lock.lock().unwrap();
                                _ = pending.fetch_add(1, Ordering::Release);
                                cvar.notify_one();
                                warn!("open-coroutine-event-loop-{i} has exited");
                            })
                            .expect("failed to spawn event-loop thread")
                    })
                    .collect()
            });
        }
    }

    pub fn stop() {
        warn!("open-coroutine is exiting...");
        EVENT_LOOP_STARTED.store(false, Ordering::Release);
        // wait for the event-loops to stop
        let (lock, cvar) = EVENT_LOOP_STOP.as_ref();
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_millis(30000),
                |stopped| {
                    stopped.load(Ordering::Acquire)
                        < EVENT_LOOP_START_COUNT.load(Ordering::Acquire) - 1
                },
            )
            .unwrap()
            .1;
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        crate::monitor::Monitor::stop();
        if result.timed_out() {
            crate::error!("open-coroutine didn't exit successfully within 30 seconds !");
        } else {
            crate::info!("open-coroutine exit successfully !");
        }
    }

    pub fn submit_co(
        f: impl FnOnce(&Suspender<(), ()>, ()) -> Option<usize> + UnwindSafe + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<CoJoinHandle> {
        Self::start();
        Self::next(true).submit_co(f, stack_size)
    }

    pub fn submit(
        f: impl FnOnce(&Suspender<(), ()>, Option<usize>) -> Option<usize> + UnwindSafe + 'static,
        param: Option<usize>,
    ) -> TaskJoinHandle {
        Self::start();
        Self::next(true).submit(f, param)
    }

    pub(crate) fn submit_raw(task: Task<'static>) {
        Self::next(true).submit_raw_task(task);
    }

    fn slice_wait_just(
        timeout: Option<Duration>,
        event_loop: &'static EventLoop,
    ) -> std::io::Result<()> {
        let timeout_time = timeout.map_or(u64::MAX, open_coroutine_timer::get_timeout_time);
        loop {
            let left_time = timeout_time
                .saturating_sub(open_coroutine_timer::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return event_loop.wait_just(Some(Duration::ZERO));
            }
            event_loop.wait_just(Some(Duration::from_nanos(left_time)))?;
        }
    }

    pub fn wait_just(timeout: Option<Duration>) -> std::io::Result<()> {
        Self::slice_wait_just(timeout, Self::next(true))
    }

    pub fn wait_read_event(fd: c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = Self::next(false);
        event_loop.add_read_event(fd)?;
        if Self::monitor().eq(event_loop) {
            // wait only happens in non-monitor for non-monitor thread
            return Self::wait_just(timeout);
        }
        Self::slice_wait_just(timeout, event_loop)
    }

    pub fn wait_write_event(fd: c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = Self::next(false);
        event_loop.add_write_event(fd)?;
        if Self::monitor().eq(event_loop) {
            // wait only happens in non-monitor for non-monitor thread
            return Self::wait_just(timeout);
        }
        Self::slice_wait_just(timeout, event_loop)
    }

    pub fn del_event(fd: c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = Self::next(false).del_event(fd);
        });
    }

    pub fn del_read_event(fd: c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = Self::next(false).del_read_event(fd);
        });
    }

    pub fn del_write_event(fd: c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = Self::next(false).del_write_event(fd);
        });
    }
}

#[allow(unused_variables, clippy::not_unsafe_ptr_arg_deref)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
impl EventLoops {
    pub fn epoll_ctl(
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> std::io::Result<c_int> {
        wrap_io_uring!(epoll_ctl, epfd, op, fd, event)
    }

    /// socket
    pub fn socket(domain: c_int, ty: c_int, protocol: c_int) -> std::io::Result<c_int> {
        wrap_io_uring!(socket, domain, ty, protocol)
    }

    pub fn accept(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> std::io::Result<c_int> {
        wrap_io_uring!(accept, fd, addr, len)
    }

    pub fn accept4(
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> std::io::Result<c_int> {
        wrap_io_uring!(accept4, fd, addr, len, flg)
    }

    pub fn connect(
        socket: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> std::io::Result<c_int> {
        wrap_io_uring!(connect, socket, address, len)
    }

    pub fn shutdown(socket: c_int, how: c_int) -> std::io::Result<c_int> {
        wrap_io_uring!(shutdown, socket, how)
    }

    pub fn close(fd: c_int) -> std::io::Result<c_int> {
        wrap_io_uring!(close, fd)
    }

    /// read
    pub fn recv(
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(recv, socket, buf, len, flags)
    }

    pub fn read(fd: c_int, buf: *mut c_void, count: size_t) -> std::io::Result<ssize_t> {
        wrap_io_uring!(read, fd, buf, count)
    }

    pub fn pread(
        fd: c_int,
        buf: *mut c_void,
        count: size_t,
        offset: off_t,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(pread, fd, buf, count, offset)
    }

    pub fn readv(fd: c_int, iov: *const iovec, iovcnt: c_int) -> std::io::Result<ssize_t> {
        wrap_io_uring!(readv, fd, iov, iovcnt)
    }

    pub fn preadv(
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(preadv, fd, iov, iovcnt, offset)
    }

    pub fn recvmsg(fd: c_int, msg: *mut msghdr, flags: c_int) -> std::io::Result<ssize_t> {
        wrap_io_uring!(recvmsg, fd, msg, flags)
    }

    /// write
    pub fn send(
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(send, socket, buf, len, flags)
    }

    pub fn sendto(
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(sendto, socket, buf, len, flags, addr, addrlen)
    }

    pub fn write(fd: c_int, buf: *const c_void, count: size_t) -> std::io::Result<ssize_t> {
        wrap_io_uring!(write, fd, buf, count)
    }

    pub fn pwrite(
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(pwrite, fd, buf, count, offset)
    }

    pub fn writev(fd: c_int, iov: *const iovec, iovcnt: c_int) -> std::io::Result<ssize_t> {
        wrap_io_uring!(writev, fd, iov, iovcnt)
    }

    pub fn pwritev(
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> std::io::Result<ssize_t> {
        wrap_io_uring!(pwritev, fd, iov, iovcnt, offset)
    }

    pub fn sendmsg(fd: c_int, msg: *const msghdr, flags: c_int) -> std::io::Result<ssize_t> {
        wrap_io_uring!(sendmsg, fd, msg, flags)
    }
}
