use crate::coroutine::suspender::Suspender;
use crate::event_loop::blocker::SelectBlocker;
use crate::event_loop::join::JoinHandle;
use crate::event_loop::selector::Selector;
use crate::pool::task::Task;
use crate::pool::CoroutinePool;
use crate::scheduler::SchedulableCoroutine;
use libc::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        use dashmap::DashMap;
        use once_cell::sync::Lazy;
        use std::sync::{Arc, Condvar, Mutex};
        use libc::{size_t, ssize_t, sockaddr, socklen_t};
    }
}

#[derive(Debug)]
pub struct EventLoop {
    cpu: u32,
    #[cfg(target_os = "linux")]
    operator: open_coroutine_iouring::io_uring::IoUringOperator,
    selector: Selector,
    //是否正在执行select
    waiting: AtomicBool,
    //协程池
    pool: MaybeUninit<CoroutinePool>,
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
            selector: Selector::new()?,
            waiting: AtomicBool::new(false),
            pool: MaybeUninit::uninit(),
        };
        let pool = CoroutinePool::new(
            stack_size,
            min_size,
            max_size,
            keep_alive_time,
            SelectBlocker::new(&mut event_loop),
        );
        event_loop.pool = MaybeUninit::new(pool);
        Ok(event_loop)
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> usize + 'static,
    ) -> JoinHandle {
        let task_name = unsafe { self.pool.assume_init_ref().submit(f) };
        JoinHandle::new(self, task_name)
    }

    pub(crate) fn submit_raw(&self, task: Task<'static>) {
        unsafe { self.pool.assume_init_ref().submit_raw(task) };
    }

    pub fn pop(&self) -> Option<Task> {
        unsafe { self.pool.assume_init_ref().pop() }
    }

    pub fn is_empty(&self) -> bool {
        unsafe { self.pool.assume_init_ref().is_empty() }
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

    pub fn add_read_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.add_read_event(fd, EventLoop::token(false))
    }

    pub fn add_write_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.add_write_event(fd, EventLoop::token(false))
    }

    pub fn del_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_event(fd)
    }

    pub fn del_read_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_read_event(fd)
    }

    pub fn del_write_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector.del_write_event(fd)
    }

    pub fn wait_just(&'static self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.wait(timeout, false)
    }

    pub fn wait_event(&'static self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.wait(timeout, true)
    }

    fn wait(
        &'static self,
        timeout: Option<Duration>,
        schedule_before_wait: bool,
    ) -> std::io::Result<()> {
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        #[allow(unused_mut)]
        let mut timeout = if schedule_before_wait {
            timeout.map(|time| {
                Duration::from_nanos(unsafe {
                    self.pool.assume_init_ref().try_timed_schedule(time)
                })
            })
        } else {
            timeout
        };

        #[cfg(target_os = "linux")]
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
        let mut events = Vec::with_capacity(1024);
        _ = self.selector.select(&mut events, timeout).map_err(|e| {
            self.waiting.store(false, Ordering::Release);
            e
        })?;
        self.waiting.store(false, Ordering::Release);
        for event in &events {
            let token = event.key;
            if event.readable || event.writable {
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
            self.pool.assume_init_ref().resume_syscall(co_name);
        }
    }

    #[must_use]
    pub fn get_result(task_name: &'static str) -> Option<usize> {
        CoroutinePool::get_result(task_name)
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
        let arc = Arc::new((Mutex::new(None), Condvar::new()));
        assert!(
            SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
            "The previous token was not retrieved in a timely manner"
        );
        self.operator
            .connect(token, socket, address, len)
            .map(|()| arc)
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
        let arc = Arc::new((Mutex::new(None), Condvar::new()));
        assert!(
            SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
            "The previous token was not retrieved in a timely manner"
        );
        self.operator
            .recv(token, socket, buf, len, flags)
            .map(|()| arc)
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
        let arc = Arc::new((Mutex::new(None), Condvar::new()));
        assert!(
            SYSCALL_WAIT_TABLE.insert(token, arc.clone()).is_none(),
            "The previous token was not retrieved in a timely manner"
        );
        self.operator
            .send(token, socket, buf, len, flags)
            .map(|()| arc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let pool = Box::leak(Box::new(EventLoop::new(0, 0, 0, 2, 0).unwrap()));
        _ = pool.submit(|_, _| {
            println!("1");
            1
        });
        _ = pool.submit(|_, _| {
            println!("2");
            2
        });
        _ = pool.wait_event(Some(Duration::from_secs(1)));
    }
}
