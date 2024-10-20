use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, SyscallState};
use crate::common::timer::TimerList;
use crate::common::work_steal::{LocalQueue, WorkStealQueue};
use crate::common::{get_timeout_time, now};
use crate::coroutine::listener::Listener;
use crate::coroutine::suspender::Suspender;
use crate::coroutine::Coroutine;
use crate::{co, impl_current_for, impl_display_by_debug, impl_for_named};
use dashmap::DashMap;
use std::collections::VecDeque;
use std::io::{Error, ErrorKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A type for Scheduler.
pub type SchedulableCoroutineState = CoroutineState<(), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableCoroutine<'s> = Coroutine<'s, (), (), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableSuspender<'s> = Suspender<'s, (), ()>;

/// The scheduler impls.
#[repr(C)]
#[derive(Debug)]
pub struct Scheduler<'s> {
    name: String,
    stack_size: AtomicUsize,
    listeners: VecDeque<&'s dyn Listener<(), Option<usize>>>,
    ready: LocalQueue<'s, SchedulableCoroutine<'s>>,
    suspend: TimerList<SchedulableCoroutine<'s>>,
    syscall: DashMap<&'s str, SchedulableCoroutine<'s>>,
    syscall_suspend: TimerList<&'s str>,
    results: DashMap<&'s str, Result<Option<usize>, &'s str>>,
}

impl Default for Scheduler<'_> {
    fn default() -> Self {
        Self::new(
            format!("open-coroutine-scheduler-{:?}", std::thread::current().id()),
            crate::common::constants::DEFAULT_STACK_SIZE,
        )
    }
}

impl Drop for Scheduler<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        _ = self
            .try_timed_schedule(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("Failed to stop scheduler {} !", self.name()));
        assert!(
            self.ready.is_empty(),
            "There are still coroutines to be carried out in the ready queue:{:#?} !",
            self.ready
        );
        assert!(
            self.suspend.is_empty(),
            "There are still coroutines to be carried out in the suspend queue:{:#?} !",
            self.suspend
        );
        assert!(
            self.syscall.is_empty(),
            "There are still coroutines to be carried out in the syscall queue:{:#?} !",
            self.syscall
        );
    }
}

impl_for_named!(Scheduler<'s>);

impl_current_for!(SCHEDULER, Scheduler<'s>);

impl_display_by_debug!(Scheduler<'s>);

impl<'s> Scheduler<'s> {
    /// Creates a new scheduler.
    #[must_use]
    pub fn new(name: String, stack_size: usize) -> Self {
        Scheduler {
            name,
            stack_size: AtomicUsize::new(stack_size),
            listeners: VecDeque::new(),
            ready: BeanFactory::get_or_default::<WorkStealQueue<SchedulableCoroutine>>(
                crate::common::constants::COROUTINE_GLOBAL_QUEUE_BEAN,
            )
            .local_queue(),
            suspend: TimerList::default(),
            syscall: DashMap::default(),
            syscall_suspend: TimerList::default(),
            results: DashMap::default(),
        }
    }

    /// Get the name of this scheduler.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the default stack size for the coroutines in this scheduler.
    /// If it has not been set, it will be [`crate::common::constants::DEFAULT_STACK_SIZE`].
    pub fn stack_size(&self) -> usize {
        self.stack_size.load(Ordering::Acquire)
    }

    /// Submit a closure to create new coroutine, then the coroutine will be push into ready queue.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// if create coroutine fails.
    pub fn submit_co(
        &self,
        f: impl FnOnce(&Suspender<(), ()>, ()) -> Option<usize> + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<()> {
        let mut co = co!(
            format!("{}@{}", self.name(), uuid::Uuid::new_v4()),
            f,
            stack_size.unwrap_or(self.stack_size()),
        )?;
        for listener in self.listeners.clone() {
            co.add_raw_listener(listener);
        }
        // let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.submit_raw_co(co)
    }

    /// Add a listener to this scheduler.
    pub fn add_listener(&mut self, listener: impl Listener<(), Option<usize>> + 's) {
        self.listeners.push_back(Box::leak(Box::new(listener)));
    }

    /// Submit a raw coroutine, then the coroutine will be push into ready queue.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    pub fn submit_raw_co(&self, coroutine: SchedulableCoroutine<'s>) -> std::io::Result<()> {
        self.ready.push_back(coroutine);
        Ok(())
    }

    /// Resume a coroutine from the system call table to the ready queue,
    /// it's generally only required for framework level crates.
    ///
    /// If we can't find the coroutine, nothing happens.
    ///
    /// # Errors
    /// if change to ready fails.
    pub fn try_resume(&self, co_name: &'s str) {
        if let Some((_, co)) = self.syscall.remove(&co_name) {
            match co.state() {
                CoroutineState::SystemCall(val, syscall, SyscallState::Suspend(_)) => {
                    co.syscall(val, syscall, SyscallState::Callback)
                        .expect("change syscall state failed");
                }
                _ => unreachable!("try_resume unexpect CoroutineState"),
            }
            self.ready.push_back(co);
        }
    }

    /// Schedule the coroutines.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_schedule(&mut self) -> std::io::Result<()> {
        self.try_timeout_schedule(u64::MAX).map(|_| ())
    }

    /// Try scheduling the coroutines for up to `dur`.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_timed_schedule(&mut self, dur: Duration) -> std::io::Result<u64> {
        self.try_timeout_schedule(get_timeout_time(dur))
    }

    /// Attempt to schedule the coroutines before the `timeout_time` timestamp.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to schedule.
    ///
    /// Returns the left time in ns.
    ///
    /// # Errors
    /// if change to ready fails.
    pub fn try_timeout_schedule(&mut self, timeout_time: u64) -> std::io::Result<u64> {
        Self::init_current(self);
        let left_time = self.do_schedule(timeout_time);
        Self::clean_current();
        left_time
    }

    fn do_schedule(&mut self, timeout_time: u64) -> std::io::Result<u64> {
        loop {
            let left_time = timeout_time.saturating_sub(now());
            if 0 == left_time {
                return Ok(0);
            }
            self.check_ready()?;
            // schedule coroutines
            if let Some(mut coroutine) = self.ready.pop_front() {
                match coroutine.resume()? {
                    CoroutineState::SystemCall((), _, state) => {
                        //挂起协程到系统调用表
                        let co_name = Box::leak(Box::from(coroutine.name()));
                        //如果已包含，说明当前系统调用还有上层父系统调用，因此直接忽略插入结果
                        _ = self.syscall.insert(co_name, coroutine);
                        if let SyscallState::Suspend(timestamp) = state {
                            self.syscall_suspend.insert(timestamp, co_name);
                        }
                    }
                    CoroutineState::Suspend((), timestamp) => {
                        if timestamp > now() {
                            //挂起协程到时间轮
                            self.suspend.insert(timestamp, coroutine);
                        } else {
                            //放入就绪队列尾部
                            self.ready.push_back(coroutine);
                        }
                    }
                    CoroutineState::Complete(result) => {
                        let co_name = Box::leak(Box::from(coroutine.name()));
                        assert!(
                            self.results.insert(co_name, Ok(result)).is_none(),
                            "not consume result"
                        );
                    }
                    CoroutineState::Error(message) => {
                        let co_name = Box::leak(Box::from(coroutine.name()));
                        assert!(
                            self.results.insert(co_name, Err(message)).is_none(),
                            "not consume result"
                        );
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            "try_timeout_schedule should never execute to here",
                        ));
                    }
                }
                continue;
            }
            return Ok(left_time);
        }
    }

    fn check_ready(&mut self) -> std::io::Result<()> {
        // Check if the elements in the suspend queue are ready
        for _ in 0..self.suspend.entry_len() {
            if let Some((exec_time, _)) = self.suspend.front() {
                if now() < *exec_time {
                    break;
                }
                if let Some((_, mut entry)) = self.suspend.pop_front() {
                    while let Some(coroutine) = entry.pop_front() {
                        coroutine.ready()?;
                        self.ready.push_back(coroutine);
                    }
                }
            }
        }
        // Check if the elements in the syscall suspend queue are ready
        for _ in 0..self.syscall_suspend.entry_len() {
            if let Some((exec_time, _)) = self.syscall_suspend.front() {
                if now() < *exec_time {
                    break;
                }
                if let Some((_, mut entry)) = self.syscall_suspend.pop_front() {
                    while let Some(co_name) = entry.pop_front() {
                        if let Some((_, co)) = self.syscall.remove(&co_name) {
                            match co.state() {
                                CoroutineState::SystemCall(
                                    val,
                                    syscall,
                                    SyscallState::Suspend(_),
                                ) => {
                                    co.syscall(val, syscall, SyscallState::Timeout)?;
                                    self.ready.push_back(co);
                                }
                                _ => unreachable!("check_ready should never execute to here"),
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
