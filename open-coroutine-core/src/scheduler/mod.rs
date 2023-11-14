use crate::common::{Current, JoinHandle, Named};
use crate::constants::{CoroutineState, DEFAULT_STACK_SIZE};
use crate::coroutine::suspender::{Suspender, SuspenderImpl};
use crate::coroutine::{Coroutine, CoroutineImpl, SimpleCoroutine, StateCoroutine};
use crate::scheduler::listener::Listener;
use once_cell::sync::Lazy;
use open_coroutine_queue::LocalQueue;
use open_coroutine_timer::TimerList;
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// A type for Scheduler.
pub type SchedulableCoroutine<'s> = CoroutineImpl<'s, (), (), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableSuspender<'s> = SuspenderImpl<'s, (), ()>;

/// Listener abstraction and impl.
pub mod listener;

/// Join impl for scheduler.
pub mod join;

mod current;

#[cfg(test)]
mod tests;

/// A trait implemented for schedulers.
pub trait Scheduler<'s, Join: JoinHandle<Self>>:
    Debug + Default + Named + Current<'s> + Listener
{
    /// Set the default stack stack size for the coroutines in this scheduler.
    /// If it has not been set, it will be `crate::constant::DEFAULT_STACK_SIZE`.
    fn set_stack_size(&self, stack_size: usize);

    /// Submit a closure to create new coroutine, then the coroutine will be push into ready queue.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// if create coroutine fails.
    fn submit_co(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, ()) -> Option<usize> + UnwindSafe + 's,
        stack_size: Option<usize>,
    ) -> std::io::Result<Join>;

    /// Resume a coroutine from the system call table to the ready queue,
    /// it's generally only required for framework level crates.
    ///
    /// If we can't find the coroutine, nothing happens.
    ///
    /// # Errors
    /// if change to ready fails.
    fn try_resume(&self, co_name: &'s str) -> std::io::Result<()>;

    /// Schedule the coroutines.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    fn try_schedule(&self) -> std::io::Result<()> {
        self.try_timeout_schedule(std::time::Duration::MAX.as_secs())
            .map(|_| ())
    }

    /// Try scheduling the coroutines for up to `dur`.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    fn try_timed_schedule(&self, dur: std::time::Duration) -> std::io::Result<u64> {
        self.try_timeout_schedule(open_coroutine_timer::get_timeout_time(dur))
    }

    /// Attempt to schedule the coroutines before the `timeout_time` timestamp.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// Returns the left time in ns.
    ///
    /// # Errors
    /// if change to ready fails.
    fn try_timeout_schedule(&self, timeout_time: u64) -> std::io::Result<u64>;

    /// Attempt to obtain coroutine result with the given `co_name`.
    fn try_get_coroutine_result(&self, co_name: &str) -> Option<Result<Option<usize>, &str>>;

    /// Returns `true` if the ready queue, suspend queue, and syscall queue are all empty.
    fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Returns the number of coroutines owned by this scheduler.
    fn size(&self) -> usize;

    /// Add a listener to this scheduler.
    fn add_listener(&mut self, listener: impl Listener + 's);
}

static mut SUSPEND_TABLE: Lazy<TimerList<SchedulableCoroutine>> = Lazy::new(TimerList::default);

static mut SYSTEM_CALL_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

#[repr(C)]
#[derive(Debug)]
pub struct SchedulerImpl<'s> {
    name: String,
    stack_size: AtomicUsize,
    ready: LocalQueue<'s, SchedulableCoroutine<'static>>,
    listeners: VecDeque<Box<dyn Listener + 's>>,
}

impl Drop for SchedulerImpl<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.ready.is_empty(),
                "there are still coroutines to be carried out !"
            );
        }
    }
}

#[allow(dead_code)]
impl<'s> SchedulerImpl<'s> {
    #[must_use]
    pub fn new(name: String, stack_size: usize) -> Self {
        let mut scheduler = SchedulerImpl {
            name,
            stack_size: AtomicUsize::new(stack_size),
            ready: LocalQueue::default(),
            listeners: VecDeque::default(),
        };
        scheduler.init();
        scheduler
    }

    fn init(&mut self) {
        #[cfg(all(unix, feature = "preemptive-schedule"))]
        self.add_listener(crate::monitor::creator::MonitorTaskCreator::default());
    }

    fn set_stack_size(&self, stack_size: usize) {
        self.stack_size.store(stack_size, Ordering::Release);
    }

    pub fn submit_co(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, ()) -> Option<usize>
            + UnwindSafe
            + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<&'s str> {
        let coroutine = SchedulableCoroutine::new(
            format!("{}|{}", self.name, Uuid::new_v4()),
            f,
            stack_size.unwrap_or(self.stack_size.load(Ordering::Acquire)),
        )?;
        assert_eq!(
            CoroutineState::Created,
            coroutine.change_state(CoroutineState::Ready)
        );
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.on_create(&coroutine);
        self.ready.push_back(coroutine);
        Ok(co_name)
    }

    fn check_ready(&self) -> std::io::Result<()> {
        unsafe {
            for _ in 0..SUSPEND_TABLE.len() {
                if let Some((exec_time, _)) = SUSPEND_TABLE.front() {
                    if open_coroutine_timer::now() < *exec_time {
                        break;
                    }
                    //移动至"就绪"队列
                    if let Some((_, mut entry)) = SUSPEND_TABLE.pop_front() {
                        for _ in 0..entry.len() {
                            if let Some(coroutine) = entry.pop_front() {
                                coroutine.ready()?;
                                //把到时间的协程加入就绪队列
                                self.ready.push_back(coroutine);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn try_schedule(&self) -> std::io::Result<()> {
        self.try_timeout_schedule(std::time::Duration::MAX.as_secs())
            .map(|_| ())
    }

    pub fn try_timed_schedule(&self, time: std::time::Duration) -> std::io::Result<u64> {
        self.try_timeout_schedule(open_coroutine_timer::get_timeout_time(time))
    }

    pub fn try_timeout_schedule(&self, timeout_time: u64) -> std::io::Result<u64> {
        self.on_schedule(timeout_time);
        loop {
            let left_time = timeout_time.saturating_sub(open_coroutine_timer::now());
            if left_time == 0 {
                return Ok(0);
            }
            self.check_ready().unwrap();
            match self.ready.pop_front() {
                Some(mut coroutine) => {
                    self.on_resume(timeout_time, &coroutine);
                    match coroutine.resume().unwrap() {
                        CoroutineState::Suspend((), timestamp) => {
                            self.on_suspend(timeout_time, &coroutine);
                            if timestamp > 0 {
                                //挂起协程到时间轮
                                unsafe { SUSPEND_TABLE.insert(timestamp, coroutine) };
                            } else {
                                //放入就绪队列尾部
                                self.ready.push_back(coroutine);
                            }
                        }
                        CoroutineState::SystemCall((), syscall, state) => {
                            self.on_syscall(timeout_time, &coroutine, syscall, state);
                            //挂起协程到系统调用表
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            //如果已包含，说明当前系统调用还有上层父系统调用，因此直接忽略插入结果
                            unsafe { _ = SYSTEM_CALL_TABLE.insert(co_name, coroutine) };
                        }
                        CoroutineState::Complete(result) => {
                            self.on_complete(timeout_time, &coroutine, result);
                        }
                        _ => unreachable!("should never execute to here"),
                    };
                }
                None => return Ok(left_time),
            }
        }
    }

    pub fn add_listener(&mut self, listener: impl Listener + 's) {
        self.listeners.push_back(Box::new(listener));
    }

    //只有框架级crate才需要使用此方法
    pub fn try_resume(&self, co_name: &'static str) -> std::io::Result<()> {
        unsafe {
            if let Some(coroutine) = SYSTEM_CALL_TABLE.remove(&co_name) {
                self.ready.push_back(coroutine);
            }
        }
        Ok(())
    }
}

impl Default for SchedulerImpl<'_> {
    fn default() -> Self {
        Self::new(Uuid::new_v4().to_string(), DEFAULT_STACK_SIZE)
    }
}
