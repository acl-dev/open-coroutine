mod event;

mod interest;

mod selector;

use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::selector::Selector;
use base_coroutine::{Coroutine, Scheduler, StackError, UserFunc};
use once_cell::sync::Lazy;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
    ) -> Result<(), StackError> {
        EventLoop::next_scheduler().submit(f, param, size)
    }

    pub fn round_robin_schedule() -> Result<(), StackError> {
        EventLoop::round_robin_timeout_schedule(u64::MAX)
    }

    pub fn round_robin_timeout_schedule(timeout_time: u64) -> Result<(), StackError> {
        for _i in 0..num_cpus::get() {
            EventLoop::next_scheduler().try_timeout_schedule(timeout_time)?;
        }
        Ok(())
    }

    pub fn round_robin_timed_schedule(timeout_time: u64) -> Result<(), StackError> {
        loop {
            if timeout_time <= timer_utils::now() {
                return Ok(());
            }
            for _i in 0..num_cpus::get() {
                EventLoop::next_scheduler().try_timeout_schedule(timeout_time)?;
            }
        }
    }

    pub fn add_read_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        if let Some(co) = Coroutine::<&'static mut c_void, &'static mut c_void>::current() {
            self.selector
                .register(fd, co.get_id(), Interest::READABLE)?;
        }
        Ok(())
    }

    pub fn add_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        if let Some(co) = Coroutine::<&'static mut c_void, &'static mut c_void>::current() {
            self.selector
                .register(fd, co.get_id(), Interest::WRITABLE)?;
        }
        Ok(())
    }

    pub fn del_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        self.selector.deregister(fd)?;
        Ok(())
    }

    pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.scheduler.syscall();
        let mut events = Events::with_capacity(1024);
        //默认1s超时
        self.selector
            .select(&mut events, Some(timeout.unwrap_or(Duration::from_secs(1))))?;
        for event in events.iter() {
            unsafe {
                self.scheduler.resume(event.token());
            }
        }
        Ok(())
    }
}
