use crate::common::{Current, JoinHandle, Named};
use crate::coroutine::suspender::{SimpleDelaySuspender, Suspender};
use crate::coroutine::Coroutine;
use crate::net::event_loop::blocker::SelectBlocker;
use crate::net::event_loop::join::{CoJoinHandleImpl, TaskJoinHandleImpl};
use crate::net::selector::has::HasSelector;
use crate::net::selector::{Event, Events, Selector, SelectorImpl};
use crate::pool::has::HasCoroutinePool;
use crate::pool::task::Task;
use crate::pool::{CoroutinePool, CoroutinePoolImpl};
use crate::scheduler::has::HasScheduler;
use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::mem::MaybeUninit;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", feature = "io_uring"))] {
        use dashmap::DashMap;
        use libc::{epoll_event, iovec, msghdr, off_t, size_t, sockaddr, socklen_t, ssize_t};
        use once_cell::sync::Lazy;
        use std::sync::{Arc, Condvar, Mutex};

        macro_rules! io_uring_impl {
            ( $invoker: expr , $syscall: ident, $($arg: expr),* $(,)* ) => {{
                let token = EventLoop::token(true);
                $invoker
                    .$syscall(token, $($arg, )*)
                    .map(|()| {
                        let arc = Arc::new((Mutex::new(None), Condvar::new()));
                        assert!(
                            SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
                            "The previous token was not retrieved in a timely manner"
                        );
                        arc
                    })
            }};
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct EventLoop {
    cpu: u32,
    #[cfg(all(target_os = "linux", feature = "io_uring"))]
    operator: open_coroutine_iouring::io_uring::IoUringOperator,
    selector: SelectorImpl,
    //是否正在执行select
    waiting: AtomicBool,
    //协程池
    pool: MaybeUninit<CoroutinePoolImpl<'static>>,
}

impl Eq for EventLoop {}

impl PartialEq for EventLoop {
    fn eq(&self, other: &Self) -> bool {
        self.get_name().eq(other.get_name())
    }
}

#[allow(clippy::type_complexity)]
#[cfg(all(target_os = "linux", feature = "io_uring"))]
static SYSCALL_WAIT_TABLE: Lazy<DashMap<usize, Arc<(Mutex<Option<ssize_t>>, Condvar)>>> =
    Lazy::new(DashMap::new);

impl EventLoop {
    pub fn new(
        cpu: u32,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
    ) -> std::io::Result<Self> {
        let mut event_loop = EventLoop {
            cpu,
            #[cfg(all(target_os = "linux", feature = "io_uring"))]
            operator: open_coroutine_iouring::io_uring::IoUringOperator::new(cpu)?,
            selector: SelectorImpl::new()?,
            waiting: AtomicBool::new(false),
            pool: MaybeUninit::uninit(),
        };
        let pool = CoroutinePoolImpl::new(
            format!("open-coroutine-event-loop-{cpu}"),
            cpu as usize,
            stack_size,
            min_size,
            max_size,
            keep_alive_time,
            SelectBlocker::new(&mut event_loop),
        );
        event_loop.pool = MaybeUninit::new(pool);
        Ok(event_loop)
    }

    pub fn submit_co(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, ()) -> Option<usize>
            + UnwindSafe
            + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<CoJoinHandleImpl> {
        let coroutine = SchedulableCoroutine::new(
            format!("{}|{}", self.get_name(), Uuid::new_v4()),
            f,
            stack_size.unwrap_or(self.get_stack_size()),
        )?;
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.submit_raw_co(coroutine)?;
        Ok(CoJoinHandleImpl::new(self, co_name))
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 'static,
        param: Option<usize>,
    ) -> TaskJoinHandleImpl {
        let name = format!("{}|{}", self.get_name(), Uuid::new_v4());
        self.submit_raw_task(Task::new(name.clone(), f, param));
        TaskJoinHandleImpl::new(self, &name)
    }

    fn token(use_thread_id: bool) -> usize {
        if let Some(co) = SchedulableCoroutine::current() {
            let boxed: &'static mut CString = Box::leak(Box::from(
                CString::new(co.get_name()).expect("build name failed!"),
            ));
            let cstr: &'static CStr = boxed.as_c_str();
            cstr.as_ptr().cast::<c_void>() as usize
        } else if use_thread_id {
            unsafe {
                cfg_if::cfg_if! {
                    if #[cfg(windows)] {
                        let thread_id = windows_sys::Win32::System::Threading::GetCurrentThread();
                    } else {
                        let thread_id = libc::pthread_self();
                    }
                }
                thread_id as usize
            }
        } else {
            0
        }
    }

    pub fn add_read(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.add_read_event(fd, EventLoop::token(false))
    }

    pub fn add_write(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.add_write_event(fd, EventLoop::token(false))
    }

    pub fn wait_event(&'static self, timeout: Option<Duration>) -> std::io::Result<()> {
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

    pub fn wait_just(&'static self, timeout: Option<Duration>) -> std::io::Result<()> {
        let mut timeout = timeout;
        if let Some(time) = timeout {
            if let Some(suspender) = SchedulableSuspender::current() {
                suspender.delay(time);
                //回来的时候等待的时间已经到了
                timeout = Some(Duration::ZERO);
            }
        }
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        #[cfg(all(target_os = "linux", feature = "io_uring"))]
        if open_coroutine_iouring::version::support_io_uring() {
            // use io_uring
            let mut result = self.operator.select(timeout).map_err(|e| {
                self.waiting.store(false, Ordering::Release);
                e
            })?;
            for cqe in &mut result.1 {
                let syscall_result = cqe.result();
                let token = cqe.user_data() as usize;
                // resolve completed read/write tasks
                if let Some((_, pair)) = SYSCALL_WAIT_TABLE.remove(&token) {
                    let (lock, cvar) = &*pair;
                    let mut pending = lock.lock().unwrap();
                    *pending = Some(syscall_result as ssize_t);
                    // notify the condvar that the value has changed.
                    cvar.notify_one();
                }
                unsafe { self.resume(token) };
            }
            if result.0 > 0 && timeout.is_some() {
                timeout = Some(Duration::ZERO);
            }
        }

        // use epoll/kevent/iocp
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, timeout).map_err(|e| {
            self.waiting.store(false, Ordering::Release);
            e
        })?;
        self.waiting.store(false, Ordering::Release);
        for event in &events {
            let token = event.get_token();
            if event.readable() || event.writable() {
                unsafe { self.resume(token) };
            }
        }
        Ok(())
    }

    unsafe fn resume(&self, token: usize) {
        if token == 0 {
            return;
        }
        if let Ok(co_name) = CStr::from_ptr((token as *const c_void).cast::<c_char>()).to_str() {
            _ = self.try_resume(co_name);
        }
    }
}

impl HasCoroutinePool<'static> for EventLoop {
    fn pool(&self) -> &CoroutinePoolImpl<'static> {
        unsafe { self.pool.assume_init_ref() }
    }

    fn pool_mut(&mut self) -> &mut CoroutinePoolImpl<'static> {
        unsafe { self.pool.assume_init_mut() }
    }
}

impl HasSelector for EventLoop {
    fn selector(&self) -> &SelectorImpl {
        &self.selector
    }
}

#[cfg(all(target_os = "linux", feature = "io_uring"))]
impl EventLoop {
    pub fn epoll_ctl(
        &self,
        epfd: c_int,
        op: c_int,
        fd: c_int,
        event: *mut epoll_event,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, epoll_ctl, epfd, op, fd, event)
    }

    /// socket
    pub fn socket(
        &self,
        domain: c_int,
        ty: c_int,
        protocol: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, socket, domain, ty, protocol)
    }

    pub fn accept(
        &self,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, accept, fd, addr, len)
    }

    pub fn accept4(
        &self,
        fd: c_int,
        addr: *mut sockaddr,
        len: *mut socklen_t,
        flg: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, accept4, fd, addr, len, flg)
    }

    pub fn connect(
        &self,
        socket: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, connect, socket, address, len)
    }

    pub fn shutdown(
        &self,
        socket: c_int,
        how: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, shutdown, socket, how)
    }

    pub fn close(&self, fd: c_int) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, close, fd)
    }

    /// read
    pub fn recv(
        &self,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, recv, socket, buf, len, flags)
    }

    /// write

    pub fn send(
        &self,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, send, socket, buf, len, flags)
    }

    pub fn sendto(
        &self,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
        addr: *const sockaddr,
        addrlen: socklen_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(
            self.operator,
            sendto,
            socket,
            buf,
            len,
            flags,
            addr,
            addrlen
        )
    }

    pub fn write(
        &self,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, write, fd, buf, count)
    }

    pub fn pwrite(
        &self,
        fd: c_int,
        buf: *const c_void,
        count: size_t,
        offset: off_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, pwrite, fd, buf, count, offset)
    }

    pub fn writev(
        &self,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, writev, fd, iov, iovcnt)
    }

    pub fn pwritev(
        &self,
        fd: c_int,
        iov: *const iovec,
        iovcnt: c_int,
        offset: off_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, pwritev, fd, iov, iovcnt, offset)
    }

    pub fn sendmsg(
        &self,
        fd: c_int,
        msg: *const msghdr,
        flags: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        io_uring_impl!(self.operator, sendmsg, fd, msg, flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let pool = Box::leak(Box::new(EventLoop::new(0, 0, 0, 2, 0).unwrap()));
        _ = pool.submit(
            |_, _| {
                println!("1");
                None
            },
            None,
        );
        _ = pool.submit(
            |_, _| {
                println!("2");
                None
            },
            None,
        );
        _ = pool.wait_event(Some(Duration::from_secs(1)));
    }
}
