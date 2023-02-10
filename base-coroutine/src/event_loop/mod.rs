pub mod event;

pub mod interest;

mod selector;

use crate::event_loop::event::Events;
use crate::event_loop::interest::Interest;
use crate::event_loop::selector::Selector;
use crate::{SchedulableCoroutine, Scheduler, UserFunc};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[repr(C)]
pub struct JoinHandle(pub libc::c_longlong);

impl JoinHandle {
    pub fn error() -> Self {
        JoinHandle(-1)
    }

    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Option<&'static mut c_void>> {
        self.timeout_at_join(timer_utils::get_timeout_time(dur))
    }

    pub fn join(self) -> std::io::Result<Option<&'static mut c_void>> {
        self.timeout_at_join(u64::MAX)
    }

    pub fn timeout_at_join(
        &self,
        timeout_time: u64,
    ) -> std::io::Result<Option<&'static mut c_void>> {
        if self.0 <= 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid coroutine id",
            ));
        }
        let mut result = Scheduler::get_result(self.0 as usize);
        while result.is_none() {
            if timeout_time <= timer_utils::now() {
                //timeout
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
            EventLoop::round_robin_timeout_schedule(timeout_time)?;
            result = Scheduler::get_result(self.0 as usize);
        }
        Ok(result.unwrap().get_result())
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
    waiting: AtomicBool,
}

unsafe impl Send for EventLoop<'_> {}

impl<'a> EventLoop<'a> {
    fn new() -> std::io::Result<EventLoop<'a>> {
        let scheduler = Box::leak(Box::new(Scheduler::new()));
        Ok(EventLoop {
            selector: Selector::new()?,
            scheduler,
            waiting: AtomicBool::new(false),
        })
    }

    pub fn next() -> &'static mut EventLoop<'static> {
        unsafe {
            let index = INDEX.fetch_add(1, Ordering::SeqCst);
            if index == usize::MAX {
                INDEX.store(1, Ordering::SeqCst);
            }
            EVENT_LOOPS.get_mut(index % EVENT_LOOPS.len()).unwrap()
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
            .map(|co_id| JoinHandle(co_id as libc::c_longlong))
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
        let results: Vec<std::io::Result<()>> = (0..unsafe { EVENT_LOOPS.len() })
            .into_par_iter()
            .map(|_| {
                let event_loop = EventLoop::next();
                let result = event_loop.scheduler.try_timeout_schedule(timeout_time);
                let left_time = timeout_time
                    .saturating_sub(timer_utils::now())
                    .min(10_000_000);
                let _ = event_loop.wait(Some(Duration::from_nanos(left_time)));
                result
            })
            .collect();
        for result in results {
            result?;
        }
        Ok(())
    }

    pub fn round_robin_del_event(fd: libc::c_int) {
        (0..unsafe { EVENT_LOOPS.len() })
            .into_par_iter()
            .for_each(|_| {
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
        (0..unsafe { EVENT_LOOPS.len() })
            .into_par_iter()
            .for_each(|_| {
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
        (0..unsafe { EVENT_LOOPS.len() })
            .into_par_iter()
            .for_each(|_| {
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

    /// 用户不应该使用此方法
    pub fn syscall(&self) -> usize {
        if let Some(co) = SchedulableCoroutine::current() {
            let co_id = co.get_id();
            self.scheduler.syscall(co_id, co);
            return co_id;
        }
        0
    }

    pub fn add_read_event(&mut self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if READABLE_TOKEN_RECORDS.contains_key(&fd) {
                return Ok(());
            }
        }
        #[cfg(not(windows))]
        unbreakable!(self.selector.register(fd, token, Interest::READABLE))?;
        #[cfg(windows)]
        self.selector.register(fd, token, Interest::READABLE)?;
        unsafe {
            READABLE_RECORDS.insert(fd);
            READABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn add_write_event(&mut self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_TOKEN_RECORDS.contains_key(&fd) {
                return Ok(());
            }
        }
        #[cfg(not(windows))]
        unbreakable!(self.selector.register(fd, token, Interest::WRITABLE))?;
        #[cfg(windows)]
        self.selector.register(fd, token, Interest::WRITABLE)?;
        unsafe {
            WRITABLE_RECORDS.insert(fd);
            WRITABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<()> {
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        let mut events = Events::with_capacity(1024);
        self.selector.select(&mut events, timeout).map_err(|e| {
            self.waiting.store(false, Ordering::Relaxed);
            e
        })?;
        self.waiting.store(false, Ordering::Relaxed);
        for event in events.iter() {
            let fd = event.fd();
            let token = event.token();
            let _ = self.scheduler.resume(token);
            unsafe {
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
        token: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_read_event(fd, token)?;
        self.wait(timeout)
    }

    pub fn wait_write_event(
        &mut self,
        fd: libc::c_int,
        token: usize,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.add_write_event(fd, token)?;
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
        assert_eq!(handle1.join().unwrap().unwrap() as *mut c_void as usize, 1);
        assert_eq!(handle2.join().unwrap().unwrap() as *mut c_void as usize, 2);
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
        let error = handle
            .timeout_join(std::time::Duration::from_nanos(0))
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            handle
                .timeout_join(std::time::Duration::from_secs(1))
                .unwrap()
                .unwrap() as *mut c_void as usize,
            3
        );
    }
}
