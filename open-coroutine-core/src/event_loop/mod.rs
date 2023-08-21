use crate::config::Config;
use crate::coroutine::suspender::Suspender;
use crate::event_loop::core::EventLoop;
use crate::event_loop::join::JoinHandle;
use crate::pool::task::Task;
use once_cell::sync::{Lazy, OnceCell};
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub mod join;

mod selector;

mod blocker;

pub mod core;

/// 做C兼容时会用到
pub type UserFunc = extern "C" fn(*const Suspender<(), ()>, usize) -> usize;

#[derive(Debug, Copy, Clone)]
pub struct EventLoops {}

#[cfg(any(target_os = "linux", windows))]
static BIND: Lazy<bool> = Lazy::new(|| unsafe { EVENT_LOOPS.len() } <= num_cpus::get());

static mut INDEX: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

static mut EVENT_LOOPS: Lazy<Box<[EventLoop]>> = Lazy::new(|| {
    let config = Config::get_instance();
    (0..config.get_event_loop_size())
        .map(|_| {
            EventLoop::new(
                config.get_stack_size(),
                config.get_min_size(),
                config.get_max_size(),
                config.get_keep_alive_time(),
            )
            .expect("init event loop failed!")
        })
        .collect()
});

static EVENT_LOOP_WORKERS: OnceCell<Box<[std::thread::JoinHandle<()>]>> = OnceCell::new();

static EVENT_LOOP_STARTED: Lazy<AtomicBool> = Lazy::new(AtomicBool::default);

static EVENT_LOOP_STOP: Lazy<Arc<(Mutex<AtomicUsize>, Condvar)>> =
    Lazy::new(|| Arc::new((Mutex::new(AtomicUsize::new(0)), Condvar::new())));

impl EventLoops {
    fn next(skip_monitor: bool) -> &'static mut EventLoop {
        unsafe {
            let mut index = INDEX.fetch_add(1, Ordering::SeqCst);
            if skip_monitor && index % EVENT_LOOPS.len() == 0 {
                INDEX.store(1, Ordering::SeqCst);
                EVENT_LOOPS.get_mut(1).expect("init event-loop-1 failed!")
            } else {
                index %= EVENT_LOOPS.len();
                EVENT_LOOPS
                    .get_mut(index)
                    .unwrap_or_else(|| panic!("init event-loop-{index} failed!"))
            }
        }
    }

    pub(crate) fn monitor() -> &'static mut EventLoop {
        //monitor线程的EventLoop固定
        unsafe {
            EVENT_LOOPS
                .get_mut(0)
                .expect("init event-loop-monitor failed!")
        }
    }

    pub(crate) fn new_condition() -> Arc<(Mutex<AtomicUsize>, Condvar)> {
        Arc::clone(&EVENT_LOOP_STOP)
    }

    fn start() {
        if EVENT_LOOP_STARTED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            //初始化event_loop线程
            _ = EVENT_LOOP_WORKERS.get_or_init(|| {
                (1..unsafe { EVENT_LOOPS.len() })
                    .map(|i| {
                        std::thread::Builder::new()
                            .name(format!("open-coroutine-event-loop-{i}"))
                            .spawn(move || {
                                #[cfg(any(target_os = "linux", windows))]
                                if *BIND {
                                    assert!(
                                        core_affinity::set_for_current(core_affinity::CoreId {
                                            id: i
                                        }),
                                        "pin event loop thread to a single CPU core failed !"
                                    );
                                }
                                let event_loop = EventLoops::next(true);
                                while EVENT_LOOP_STARTED.load(Ordering::Acquire)
                                    || !event_loop.is_empty()
                                {
                                    _ = event_loop.wait_event(Some(Duration::from_millis(10)));
                                }
                                crate::warn!("open-coroutine-event-loop-{i} has exited");
                                let pair = EventLoops::new_condition();
                                let (lock, cvar) = pair.as_ref();
                                let pending = lock.lock().unwrap();
                                _ = pending.fetch_add(1, Ordering::Release);
                                cvar.notify_one();
                            })
                            .expect("failed to spawn event-loop thread")
                    })
                    .collect()
            });
        }
    }

    pub fn stop() {
        crate::warn!("open-coroutine is exiting...");
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        crate::monitor::Monitor::stop();
        EVENT_LOOP_STARTED.store(false, Ordering::Release);
        // wait for the event-loops to stop
        let (lock, cvar) = EVENT_LOOP_STOP.as_ref();
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_millis(30000),
                |stopped| stopped.load(Ordering::Acquire) < unsafe { EVENT_LOOPS.len() },
            )
            .unwrap()
            .1;
        if result.timed_out() {
            crate::error!("open-coroutine didn't exit successfully within 30 seconds !");
        } else {
            crate::info!("open-coroutine exit successfully !");
        }
    }

    /// todo This is actually an API for creating tasks, adding an API for creating coroutines
    pub fn submit(f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> usize + 'static) -> JoinHandle {
        EventLoops::start();
        EventLoops::next(true).submit(f)
    }

    pub(crate) fn submit_raw(task: Task<'static>) {
        EventLoops::next(true).submit_raw(task);
    }

    fn slice_wait(
        timeout: Option<Duration>,
        event_loop: &'static mut EventLoop,
    ) -> std::io::Result<()> {
        let time = timeout.unwrap_or(Duration::MAX);
        if let Some(suspender) = Suspender::<(), ()>::current() {
            suspender.delay(time);
            //回来的时候等待的时间已经到了
            return event_loop.wait_just(Some(Duration::ZERO));
        }
        let timeout_time = open_coroutine_timer::get_timeout_time(time);
        loop {
            let left_time = timeout_time
                .saturating_sub(open_coroutine_timer::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return event_loop.wait_just(Some(Duration::ZERO));
            }
            event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
        }
    }

    pub fn wait_event(timeout: Option<Duration>) -> std::io::Result<()> {
        Self::slice_wait(timeout, EventLoops::next(false))
    }

    pub fn wait_read_event(fd: libc::c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = EventLoops::next(false);
        event_loop.add_read_event(fd)?;
        Self::slice_wait(timeout, event_loop)
    }

    pub fn wait_write_event(fd: libc::c_int, timeout: Option<Duration>) -> std::io::Result<()> {
        let event_loop = EventLoops::next(false);
        event_loop.add_write_event(fd)?;
        Self::slice_wait(timeout, event_loop)
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
