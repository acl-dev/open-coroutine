use crate::common::{Current, JoinHandle, Named};
use crate::coroutine::suspender::Suspender;
use crate::coroutine::Coroutine;
use crate::net::event_loop::blocker::SelectBlocker;
use crate::net::event_loop::join::{CoJoinHandleImpl, TaskJoinHandleImpl};
use crate::net::selector::has::HasSelector;
use crate::net::selector::{Event, Events, Selector, SelectorImpl};
use crate::pool::has::HasCoroutinePool;
use crate::pool::task::Task;
use crate::pool::{CoroutinePool, CoroutinePoolImpl};
use crate::scheduler::has::HasScheduler;
use crate::scheduler::SchedulableCoroutine;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::mem::MaybeUninit;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        use dashmap::DashMap;
        use once_cell::sync::Lazy;
        use std::sync::{Arc, Condvar, Mutex};
        use libc::{size_t, ssize_t, sockaddr, socklen_t};
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct EventLoop {
    cpu: u32,
    #[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
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
            #[cfg(target_os = "linux")]
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
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {
                let mut timeout = timeout;
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

#[cfg(target_os = "linux")]
impl EventLoop {
    /// socket
    pub fn connect(
        &self,
        socket: c_int,
        address: *const sockaddr,
        len: socklen_t,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        let token = EventLoop::token(true);
        self.operator
            .connect(token, socket, address, len)
            .map(|()| {
                let arc = Arc::new((Mutex::new(None), Condvar::new()));
                assert!(
                    SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
                    "The previous token was not retrieved in a timely manner"
                );
                arc
            })
    }

    /// read
    pub fn recv(
        &self,
        socket: c_int,
        buf: *mut c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        let token = EventLoop::token(true);
        self.operator
            .recv(token, socket, buf, len, flags)
            .map(|()| {
                let arc = Arc::new((Mutex::new(None), Condvar::new()));
                assert!(
                    SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
                    "The previous token was not retrieved in a timely manner"
                );
                arc
            })
    }

    /// write

    pub fn send(
        &self,
        socket: c_int,
        buf: *const c_void,
        len: size_t,
        flags: c_int,
    ) -> std::io::Result<Arc<(Mutex<Option<ssize_t>>, Condvar)>> {
        let token = EventLoop::token(true);
        self.operator
            .send(token, socket, buf, len, flags)
            .map(|()| {
                let arc = Arc::new((Mutex::new(None), Condvar::new()));
                assert!(
                    SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
                    "The previous token was not retrieved in a timely manner"
                );
                arc
            })
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
