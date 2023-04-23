use crate::coroutine::suspender::Suspender;
use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::join::JoinHandle;
use crate::event_loop::selector::Selector;
use crate::scheduler::Scheduler;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub mod join;

pub mod event;

pub mod interest;

mod selector;

#[derive(Debug)]
pub struct EventLoop {
    selector: Selector,
    scheduler: Scheduler,
    waiting: AtomicBool,
}

static mut READABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut READABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut WRITABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut WRITABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

impl EventLoop {
    pub fn new() -> std::io::Result<Self> {
        Ok(EventLoop {
            selector: Selector::new()?,
            scheduler: Scheduler::new(),
            waiting: AtomicBool::new(false),
        })
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> &'static mut c_void + 'static,
    ) -> std::io::Result<JoinHandle> {
        self.scheduler
            .submit(f)
            .map(|co_name| JoinHandle::new(Some(self), co_name))
    }

    pub fn add_read_event(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if READABLE_TOKEN_RECORDS.contains_key(&fd) {
                return Ok(());
            }
        }
        self.selector.register(fd, token, Interest::READABLE)?;
        unsafe {
            assert!(READABLE_RECORDS.insert(fd));
            assert_eq!(None, READABLE_TOKEN_RECORDS.insert(fd, token));
        }
        Ok(())
    }

    pub fn add_write_event(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_TOKEN_RECORDS.contains_key(&fd) {
                return Ok(());
            }
        }
        self.selector.register(fd, token, Interest::WRITABLE)?;
        unsafe {
            assert!(WRITABLE_RECORDS.insert(fd));
            assert_eq!(None, WRITABLE_TOKEN_RECORDS.insert(fd, token));
        }
        Ok(())
    }

    pub fn wait_event(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.wait(timeout, true)
    }

    pub fn wait(
        &self,
        timeout: Option<Duration>,
        schedule_before_wait: bool,
    ) -> std::io::Result<()> {
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        let timeout = if schedule_before_wait {
            timeout.map(|time| Duration::from_nanos(self.scheduler.try_timed_schedule(time)))
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
            self.scheduler.resume_syscall(token);
            unsafe {
                if event.is_readable() {
                    assert!(READABLE_TOKEN_RECORDS.remove(&fd).is_some());
                }
                if event.is_writable() {
                    assert!(WRITABLE_TOKEN_RECORDS.remove(&fd).is_some());
                }
            }
        }
        Ok(())
    }

    pub fn wait_read_event(
        &self,
        fd: libc::c_int,
        token: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_read_event(fd, token)?;
        self.wait_event(timeout)
    }

    pub fn wait_write_event(
        &self,
        fd: libc::c_int,
        token: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_write_event(fd, token)?;
        self.wait_event(timeout)
    }
}
