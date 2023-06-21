use crate::coroutine::suspender::Suspender;
use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::join::JoinHandle;
use crate::event_loop::selector::Selector;
use crate::event_loop::task::Task;
use crate::scheduler::listener::Listener;
use crate::scheduler::{SchedulableCoroutine, Scheduler};
use crossbeam_deque::{Injector, Steal};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub struct EventLoop {
    selector: Selector,
    //是否正在执行select
    waiting: AtomicBool,
    //工作协程
    workers: Scheduler,
    //协程栈大小
    stack_size: usize,
    //当前协程数
    running: AtomicUsize,
    //最小协程数，即核心协程数
    min_size: usize,
    //最大协程数
    max_size: usize,
    //队列
    work_queue: Injector<Task<'static>>,
    //非核心协程的最大存活时间，单位ns
    keep_alive_time: u64,
    //是否向workers注册监听器
    register: AtomicBool,
}

static mut READABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut READABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut WRITABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut WRITABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut RESULT_TABLE: Lazy<HashMap<&str, usize>> = Lazy::new(HashMap::new);

impl EventLoop {
    pub fn new(
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
    ) -> std::io::Result<Self> {
        Ok(EventLoop {
            selector: Selector::new()?,
            waiting: AtomicBool::new(false),
            workers: Scheduler::new(),
            stack_size,
            running: AtomicUsize::new(0),
            min_size,
            max_size,
            work_queue: Injector::default(),
            keep_alive_time,
            register: AtomicBool::new(false),
        })
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> usize + 'static,
    ) -> JoinHandle {
        let name: Box<str> = Box::from(Uuid::new_v4().to_string());
        let clone = Box::leak(name.clone());
        self.work_queue.push(Task::new(name, f));
        JoinHandle::new(self, clone)
    }

    pub fn grow(&'static self) -> std::io::Result<()> {
        if self.work_queue.is_empty() {
            return Ok(());
        }
        if self.running.load(Ordering::Acquire) >= self.max_size {
            return Ok(());
        }
        let create_time = open_coroutine_timer::now();
        _ = self.workers.submit(
            move |suspender, _| {
                loop {
                    match self.work_queue.steal() {
                        Steal::Empty => {
                            let larger_than_min =
                                self.running.load(Ordering::Acquire) > self.min_size;
                            let keep_alive =
                                open_coroutine_timer::now() - create_time < self.keep_alive_time;
                            if larger_than_min && !keep_alive {
                                //回收worker协程
                                return 0;
                            }
                            suspender.delay(Duration::from_millis(10));
                        }
                        Steal::Success(task) => {
                            let task_name = task.get_name();
                            let result = task.run(suspender);
                            unsafe { assert!(RESULT_TABLE.insert(task_name, result).is_none()) }
                        }
                        Steal::Retry => continue,
                    }
                }
            },
            if self.stack_size > 0 {
                Some(self.stack_size)
            } else {
                None
            },
        )?;
        _ = self.running.fetch_add(1, Ordering::Release);
        Ok(())
    }

    fn token() -> usize {
        if let Some(co) = SchedulableCoroutine::current() {
            let boxed: &'static mut CString =
                Box::leak(Box::from(CString::new(co.get_name()).unwrap()));
            let cstr: &'static CStr = boxed.as_c_str();
            cstr.as_ptr().cast::<c_void>() as usize
        } else {
            0
        }
    }

    pub fn add_read_event(&self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
            let token = EventLoop::token();
            if WRITABLE_RECORDS.contains(&fd) {
                //同时对读写事件感兴趣
                let interests = Interest::READABLE.add(Interest::WRITABLE);
                self.selector
                    .reregister(fd, token, interests)
                    .or(self.selector.register(fd, token, interests))
            } else {
                self.selector.register(fd, token, Interest::READABLE)
            }?;
            _ = READABLE_RECORDS.insert(fd);
            _ = READABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn add_write_event(&self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
            let token = EventLoop::token();
            if READABLE_RECORDS.contains(&fd) {
                //同时对读写事件感兴趣
                let interests = Interest::WRITABLE.add(Interest::READABLE);
                self.selector
                    .reregister(fd, token, interests)
                    .or(self.selector.register(fd, token, interests))
            } else {
                self.selector.register(fd, token, Interest::WRITABLE)
            }?;
            _ = WRITABLE_RECORDS.insert(fd);
            _ = WRITABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn del_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            let token = READABLE_TOKEN_RECORDS
                .remove(&fd)
                .or(WRITABLE_TOKEN_RECORDS.remove(&fd))
                .unwrap_or(0);
            self.selector.deregister(fd, token)?;
            _ = READABLE_RECORDS.remove(&fd);
            _ = WRITABLE_RECORDS.remove(&fd);
        }
        Ok(())
    }

    pub fn del_read_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                if WRITABLE_RECORDS.contains(&fd) {
                    //写事件不能删
                    self.selector.reregister(
                        fd,
                        *WRITABLE_TOKEN_RECORDS.get(&fd).unwrap_or(&0),
                        Interest::WRITABLE,
                    )?;
                    assert!(READABLE_RECORDS.remove(&fd));
                    assert!(READABLE_TOKEN_RECORDS.remove(&fd).is_some());
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
    }

    pub fn del_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                if READABLE_RECORDS.contains(&fd) {
                    //读事件不能删
                    self.selector.reregister(
                        fd,
                        *READABLE_TOKEN_RECORDS.get(&fd).unwrap_or(&0),
                        Interest::READABLE,
                    )?;
                    assert!(WRITABLE_RECORDS.remove(&fd));
                    assert!(WRITABLE_TOKEN_RECORDS.remove(&fd).is_some());
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
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
        _ = self.grow();
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        if self
            .register
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            #[derive(Debug)]
            struct CoroutineListener {
                s: &'static EventLoop,
            }

            impl Listener for CoroutineListener {
                fn on_suspend(&self, _co: &SchedulableCoroutine) {
                    _ = self.s.grow();
                }
                fn on_syscall(&self, _co: &SchedulableCoroutine, _syscall_name: &str) {
                    _ = self.s.grow();
                }
            }
            self.workers.add_listener(CoroutineListener { s: self });
        }
        let timeout = if schedule_before_wait {
            timeout.map(|time| Duration::from_nanos(self.workers.try_timed_schedule(time)))
        } else {
            timeout
        };
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, timeout).map_err(|e| {
            self.waiting.store(false, Ordering::Relaxed);
            e
        })?;
        self.waiting.store(false, Ordering::Relaxed);
        for event in events.iter() {
            let fd = event.fd();
            let token = event.token();
            unsafe {
                if event.is_readable() {
                    _ = READABLE_TOKEN_RECORDS.remove(&fd);
                    self.resume(token);
                }
                if event.is_writable() {
                    _ = WRITABLE_TOKEN_RECORDS.remove(&fd);
                    self.resume(token);
                }
            }
        }
        Ok(())
    }

    unsafe fn resume(&self, token: usize) {
        if token == 0 {
            return;
        }
        if let Ok(co_name) = CStr::from_ptr((token as *const c_void).cast::<c_char>()).to_str() {
            self.workers.resume_syscall(co_name);
        }
    }

    pub fn get_result(co_name: &'static str) -> Option<usize> {
        unsafe { RESULT_TABLE.remove(&co_name) }
    }
}

impl Default for EventLoop {
    fn default() -> Self {
        EventLoop::new(crate::coroutine::default_stack_size(), 0, 65536, 0)
            .expect("init event loop failed!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let pool = Box::leak(Box::new(EventLoop::new(0, 0, 2, 0).unwrap()));
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
