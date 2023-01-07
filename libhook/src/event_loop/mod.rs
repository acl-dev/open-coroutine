mod event;

mod interest;

mod selector;

use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::selector::Selector;
use base_coroutine::{Coroutine, Scheduler, UserFunc};
use once_cell::sync::Lazy;
use std::io::ErrorKind;
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
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::READABLE)?;
        Ok(())
    }

    pub fn add_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::WRITABLE)?;
        Ok(())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.scheduler.syscall();
        let mut events = Events::with_capacity(1024);
        if let Err(e) = self.selector.select(&mut events, timeout) {
            match e.kind() {
                //maybe invoke by Monitor::signal(), just ignore this
                ErrorKind::Interrupted => {}
                _ => return Err(e),
            }
        }
        for event in events.iter() {
            let _ = unsafe { self.scheduler.resume(event.token()) };
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
