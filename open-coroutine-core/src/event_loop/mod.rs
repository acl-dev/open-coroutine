use crate::coroutine::suspender::Suspender;
use crate::event_loop::core::EventLoop;
use crate::event_loop::join::JoinHandle;
use once_cell::sync::{Lazy, OnceCell};
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

pub mod join;

pub mod event;

pub mod interest;

mod selector;

mod task;

pub mod core;

/// 做C兼容时会用到
pub type UserFunc = extern "C" fn(*const Suspender<(), ()>, usize) -> usize;

#[derive(Debug, Copy, Clone)]
pub struct EventLoops {}

static mut INDEX: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

// todo 增加配置类，允许用户配置一些参数
static mut EVENT_LOOPS: Lazy<Box<[EventLoop]>> =
    Lazy::new(|| (0..num_cpus::get()).map(|_| EventLoop::default()).collect());

static EVENT_LOOP_WORKERS: OnceCell<Box<[std::thread::JoinHandle<()>]>> = OnceCell::new();

static EVENT_LOOP_STARTED: Lazy<AtomicBool> = Lazy::new(AtomicBool::default);

impl EventLoops {
    fn next(skip_monitor: bool) -> &'static mut EventLoop {
        unsafe {
            let index = INDEX.fetch_add(1, Ordering::SeqCst);
            if skip_monitor && index % EVENT_LOOPS.len() == 0 {
                INDEX.store(1, Ordering::SeqCst);
                EVENT_LOOPS.get_mut(1).unwrap()
            } else {
                EVENT_LOOPS.get_mut(index % EVENT_LOOPS.len()).unwrap()
            }
        }
    }

    pub(crate) fn monitor() -> &'static mut EventLoop {
        //monitor线程的EventLoop固定
        unsafe { EVENT_LOOPS.get_mut(0).unwrap() }
    }

    pub fn start() {
        if EVENT_LOOP_STARTED
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            //初始化event_loop线程
            _ = EVENT_LOOP_WORKERS.get_or_init(|| {
                (1..unsafe { EVENT_LOOPS.len() })
                    .map(|_| {
                        std::thread::spawn(|| {
                            let event_loop = EventLoops::next(true);
                            while EVENT_LOOP_STARTED.load(Ordering::Acquire) {
                                _ = event_loop.wait_event(Some(Duration::from_millis(10)));
                            }
                        })
                    })
                    .collect()
            });
        }
    }

    pub fn stop() {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        crate::monitor::Monitor::stop();
        EVENT_LOOP_STARTED.store(false, Ordering::Release);
    }

    /// todo This is actually an API for creating tasks, adding an API for creating coroutines
    pub fn submit(f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> usize + 'static) -> JoinHandle {
        EventLoops::start();
        EventLoops::next(true).submit(f)
    }

    fn slice_wait(time: Duration, event_loop: &'static mut EventLoop) -> std::io::Result<()> {
        let timeout_time = open_coroutine_timer::get_timeout_time(time);
        loop {
            let left_time = timeout_time
                .saturating_sub(open_coroutine_timer::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return Ok(());
            }
            event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
        }
    }

    pub fn wait_event(timeout: Option<Duration>) -> std::io::Result<()> {
        let time = timeout.unwrap_or(Duration::MAX);
        if let Some(suspender) = Suspender::<(), ()>::current() {
            suspender.delay(time);
            return Ok(());
        }
        Self::slice_wait(time, EventLoops::next(false))
    }

    pub fn wait_read_event(fd: libc::c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = EventLoops::next(false);
        event_loop.add_read_event(fd)?;
        let time = timeout.unwrap_or(Duration::MAX);
        if let Some(suspender) = Suspender::<(), ()>::current() {
            suspender.delay(time);
            //回来的时候事件已经发生了
            return Ok(());
        }
        Self::slice_wait(time, event_loop)
    }

    pub fn wait_write_event(fd: libc::c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = EventLoops::next(false);
        event_loop.add_write_event(fd)?;
        let time = timeout.unwrap_or(Duration::MAX);
        if let Some(suspender) = Suspender::<(), ()>::current() {
            suspender.delay(time);
            //回来的时候事件已经发生了
            return Ok(());
        }
        Self::slice_wait(time, event_loop)
    }

    pub fn del_event(fd: libc::c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = EventLoops::next(false).del_event(fd);
        });
    }

    pub fn del_read_event(fd: libc::c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = EventLoops::next(false).del_read_event(fd);
        });
    }

    pub fn del_write_event(fd: libc::c_int) {
        (0..unsafe { EVENT_LOOPS.len() }).for_each(|_| {
            _ = EventLoops::next(false).del_write_event(fd);
        });
    }
}
