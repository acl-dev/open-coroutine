pub mod event;

pub mod interest;

mod selector;

use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::selector::Selector;
use crate::{Coroutine, Scheduler, UserFunc};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static mut READABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut READABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut WRITABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut WRITABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut INDEX: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

static mut EVENT_LOOPS: Lazy<Box<[EventLoop]>> = Lazy::new(|| {
    (0..num_cpus::get())
        .map(|_| EventLoop::new().expect("init event loop failed!"))
        .collect()
});

pub struct EventLoop<'a> {
    selector: Selector,
    scheduler: &'a mut Scheduler,
}

unsafe impl Send for EventLoop<'_> {}

impl<'a> EventLoop<'a> {
    fn new() -> std::io::Result<EventLoop<'a>> {
        let scheduler = Box::leak(Box::new(Scheduler::new()));
        Ok(EventLoop {
            selector: Selector::new()?,
            scheduler,
        })
    }

    pub fn next() -> &'static mut EventLoop<'static> {
        unsafe {
            let index = INDEX.fetch_add(1, Ordering::SeqCst);
            if index == usize::MAX {
                INDEX.store(1, Ordering::SeqCst);
            }
            EVENT_LOOPS.get_mut(index % num_cpus::get()).unwrap()
        }
    }

    fn next_scheduler() -> &'static mut Scheduler {
        EventLoop::next().scheduler
    }

    pub fn submit(
        f: UserFunc<&'static mut c_void, (), &'static mut c_void>,
        param: &'static mut c_void,
        size: usize,
    ) -> std::io::Result<()> {
        EventLoop::next_scheduler().submit(f, param, size)
    }

    pub fn round_robin_schedule() -> std::io::Result<()> {
        EventLoop::round_robin_timeout_schedule(u64::MAX)
    }

    pub fn round_robin_timeout_schedule(timeout_time: u64) -> std::io::Result<()> {
        for _i in 0..num_cpus::get() {
            EventLoop::next_scheduler().try_timeout_schedule(timeout_time)?;
        }
        Ok(())
    }

    pub fn round_robin_timed_schedule(timeout_time: u64) -> std::io::Result<()> {
        loop {
            if timeout_time <= timer_utils::now() {
                return Ok(());
            }
            for _i in 0..num_cpus::get() {
                EventLoop::next_scheduler().try_timeout_schedule(timeout_time)?;
            }
        }
    }

    pub fn round_robin_del_event(fd: libc::c_int) {
        for _i in 0..num_cpus::get() {
            let _ = EventLoop::next().del_event(fd);
        }
    }

    fn del_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        self.selector.deregister(fd)?;
        unsafe {
            READABLE_RECORDS.remove(&fd);
            READABLE_TOKEN_RECORDS.remove(&fd);
            WRITABLE_RECORDS.remove(&fd);
            WRITABLE_TOKEN_RECORDS.remove(&fd);
        }
        Ok(())
    }

    pub fn round_robin_del_read_event(fd: libc::c_int) {
        for _i in 0..num_cpus::get() {
            let _ = EventLoop::next().del_read_event(fd);
        }
    }

    fn del_read_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                if WRITABLE_RECORDS.contains(&fd) {
                    //写事件不能删
                    self.selector.reregister(
                        fd,
                        WRITABLE_TOKEN_RECORDS.remove(&fd).unwrap_or(0),
                        Interest::WRITABLE,
                    )?;
                    READABLE_RECORDS.remove(&fd);
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
    }

    pub fn round_robin_del_write_event(fd: libc::c_int) {
        for _i in 0..num_cpus::get() {
            let _ = EventLoop::next().del_write_event(fd);
        }
    }

    fn del_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                if READABLE_RECORDS.contains(&fd) {
                    //读事件不能删
                    self.selector.reregister(
                        fd,
                        READABLE_TOKEN_RECORDS.remove(&fd).unwrap_or(0),
                        Interest::READABLE,
                    )?;
                    WRITABLE_RECORDS.remove(&fd);
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
    }

    fn build_token() -> usize {
        if let Some(co) = Coroutine::<&'static mut c_void, &'static mut c_void>::current() {
            co.get_id()
        } else {
            0
        }
    }

    pub fn add_read_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
        }
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::READABLE)?;
        unsafe {
            READABLE_RECORDS.insert(fd);
            READABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn add_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
        }
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::WRITABLE)?;
        unsafe {
            WRITABLE_RECORDS.insert(fd);
            WRITABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.scheduler.syscall();
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, timeout)?;
        for event in events.iter() {
            let fd = event.fd();
            let token = event.token();
            unsafe {
                let _ = self.scheduler.resume(token);
                if event.is_readable() {
                    READABLE_RECORDS.remove(&fd);
                    READABLE_TOKEN_RECORDS.remove(&fd);
                }
                if event.is_writable() {
                    WRITABLE_RECORDS.remove(&fd);
                    WRITABLE_TOKEN_RECORDS.remove(&fd);
                }
            }
        }
        Ok(())
    }

    pub fn wait_read_event(
        &mut self,
        fd: libc::c_int,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_read_event(fd)?;
        self.wait(timeout)
    }

    pub fn wait_write_event(
        &mut self,
        fd: libc::c_int,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_write_event(fd)?;
        self.wait(timeout)
    }
}
