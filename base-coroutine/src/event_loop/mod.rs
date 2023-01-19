pub mod event;

pub mod interest;

mod selector;

use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::selector::Selector;
use crate::{Coroutine, Scheduler, UserFunc};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[repr(C)]
pub struct JoinHandle(pub &'static c_void);

impl JoinHandle {
    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Option<usize>> {
        if self.0 as *const c_void as usize == 0 {
            return Ok(Some(0));
        }
        let timeout_time = timer_utils::get_timeout_time(dur);
        let result = unsafe {
            &*(self.0 as *const _ as *const Coroutine<&'static mut c_void, &'static mut c_void>)
        };
        while result.get_result().is_none() {
            if timeout_time <= timer_utils::now() {
                //timeout
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
            EventLoop::round_robin_timeout_schedule(timeout_time)?;
            if result.get_result().is_some() {
                break;
            }
            let left_time = timeout_time.saturating_sub(timer_utils::now());
            //等待事件到来
            if let Err(e) = EventLoop::next().wait(Some(Duration::from_nanos(left_time))) {
                match e.kind() {
                    //maybe invoke by Monitor::signal(), just ignore this
                    std::io::ErrorKind::Interrupted => continue,
                    _ => return Err(e),
                }
            }
        }
        Ok(result.get_result().map(|ptr| ptr as *mut c_void as usize))
    }

    pub fn join(self) -> std::io::Result<usize> {
        if self.0 as *const c_void as usize == 0 {
            return Ok(0);
        }
        let result = unsafe {
            &*(self.0 as *const _ as *const Coroutine<&'static mut c_void, &'static mut c_void>)
        };
        while result.get_result().is_none() {
            EventLoop::round_robin_schedule()?;
            if result.get_result().is_some() {
                break;
            }
            //等待事件到来
            if let Err(e) = EventLoop::next().wait(Some(Duration::from_secs(1))) {
                match e.kind() {
                    //maybe invoke by Monitor::signal(), just ignore this
                    std::io::ErrorKind::Interrupted => continue,
                    _ => return Err(e),
                }
            }
        }
        Ok(result
            .get_result()
            .map(|co| co as *mut c_void as usize)
            .unwrap())
    }
}

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
    ) -> std::io::Result<JoinHandle> {
        EventLoop::next_scheduler()
            .submit(f, param, size)
            .map(|co| JoinHandle(unsafe { std::mem::transmute(co) }))
    }

    pub fn round_robin_schedule() -> std::io::Result<()> {
        EventLoop::round_robin_timeout_schedule(u64::MAX)
    }

    pub fn round_robin_timed_schedule(timeout_time: u64) -> std::io::Result<()> {
        loop {
            if timeout_time <= timer_utils::now() {
                return Ok(());
            }
            EventLoop::round_robin_timeout_schedule(timeout_time)?;
        }
    }

    pub fn round_robin_timeout_schedule(timeout_time: u64) -> std::io::Result<()> {
        let results: Vec<std::io::Result<()>> = (0..num_cpus::get())
            .into_par_iter()
            .map(|_| EventLoop::next_scheduler().try_timeout_schedule(timeout_time))
            .collect();
        for result in results {
            result?;
        }
        Ok(())
    }

    pub fn round_robin_del_event(fd: libc::c_int) {
        (0..num_cpus::get()).into_par_iter().for_each(|_| {
            let _ = EventLoop::next().del_event(fd);
        });
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
        (0..num_cpus::get()).into_par_iter().for_each(|_| {
            let _ = EventLoop::next().del_read_event(fd);
        });
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
        (0..num_cpus::get()).into_par_iter().for_each(|_| {
            let _ = EventLoop::next().del_write_event(fd);
        });
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
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::READABLE)?;
        unsafe {
            READABLE_RECORDS.insert(fd);
            READABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn add_write_event(&mut self, fd: libc::c_int) -> std::io::Result<()> {
        let token = <EventLoop<'a>>::build_token();
        self.selector.register(fd, token, Interest::WRITABLE)?;
        unsafe {
            WRITABLE_RECORDS.insert(fd);
            WRITABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        //fixme 这里应该只调1次scheduler.syscall，实际由于外层的loop，可能会调用多次
        self.scheduler.syscall();
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, timeout)?;
        for event in events.iter() {
            let fd = event.fd();
            let token = event.token();
            unsafe {
                let _ = self.scheduler.resume(token);
                if event.is_readable() {
                    READABLE_TOKEN_RECORDS.remove(&fd);
                }
                if event.is_writable() {
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

#[cfg(test)]
mod tests {
    use crate::{EventLoop, Yielder};
    use std::os::raw::c_void;

    fn val(val: usize) -> &'static mut c_void {
        unsafe { std::mem::transmute(val) }
    }

    extern "C" fn f1(
        _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
        input: &'static mut c_void,
    ) -> &'static mut c_void {
        println!("[coroutine1] launched");
        input
    }

    extern "C" fn f2(
        _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
        input: &'static mut c_void,
    ) -> &'static mut c_void {
        println!("[coroutine2] launched");
        input
    }

    #[test]
    fn join_test() {
        let handle1 = EventLoop::submit(f1, val(1), 4096).expect("submit failed !");
        let handle2 = EventLoop::submit(f2, val(2), 4096).expect("submit failed !");
        assert_eq!(handle1.join().unwrap(), 1);
        assert_eq!(handle2.join().unwrap(), 2);
    }

    extern "C" fn f3(
        _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
        input: &'static mut c_void,
    ) -> &'static mut c_void {
        println!("[coroutine3] launched");
        input
    }

    #[test]
    fn timed_join_test() {
        let handle = EventLoop::submit(f3, val(3), 4096).expect("submit failed !");
        assert_eq!(
            handle
                .timeout_join(std::time::Duration::from_nanos(0))
                .unwrap(),
            None
        );
        assert_eq!(
            handle
                .timeout_join(std::time::Duration::from_secs(1))
                .unwrap(),
            Some(3)
        );
    }
}
