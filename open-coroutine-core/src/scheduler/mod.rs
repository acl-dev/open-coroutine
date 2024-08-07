use crate::common::{Current, JoinHandler, Named};
use crate::constants::{CoroutineState, SyscallState};
use crate::coroutine::listener::Listener;
use crate::coroutine::suspender::Suspender;
use crate::coroutine::Coroutine;
use crate::scheduler::join::JoinHandle;
use crate::{error, impl_current_for, impl_for_named};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use open_coroutine_queue::LocalQueue;
use open_coroutine_timer::TimerList;
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::ops::Deref;
use std::panic::UnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uuid::Uuid;

/// A type for Scheduler.
pub type SchedulableCoroutineState = CoroutineState<(), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableCoroutine<'s> = Coroutine<'s, (), (), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableSuspender<'s> = Suspender<'s, (), ()>;

/// A type for Scheduler.
pub trait SchedulableListener: Listener<(), (), Option<usize>> {}

/// Join impl for scheduler.
pub mod join;

#[cfg(test)]
mod tests;

static mut SUSPEND_TABLE: Lazy<TimerList<SchedulableCoroutine>> = Lazy::new(TimerList::default);

static mut SYSTEM_CALL_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

static mut SYSTEM_CALL_SUSPEND_TABLE: Lazy<TimerList<&str>> = Lazy::new(TimerList::default);

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler<'s> {
    name: String,
    scheduling: AtomicBool,
    stack_size: AtomicUsize,
    ready: LocalQueue<'s, SchedulableCoroutine<'static>>,
    results: DashMap<&'s str, Result<Option<usize>, &'s str>>,
    listeners: VecDeque<&'s dyn SchedulableListener>,
}

impl<'s> Scheduler<'s> {
    #[must_use]
    pub fn new(name: String, stack_size: usize) -> Self {
        Scheduler {
            name,
            scheduling: AtomicBool::new(false),
            stack_size: AtomicUsize::new(stack_size),
            ready: LocalQueue::default(),
            results: DashMap::default(),
            listeners: VecDeque::default(),
        }
    }

    /// Get the default stack size for the coroutines in this scheduler.
    /// If it has not been set, it will be `crate::constant::DEFAULT_STACK_SIZE`.
    pub fn get_stack_size(&self) -> usize {
        self.stack_size.load(Ordering::Acquire)
    }

    /// Set the default stack size for the coroutines in this scheduler.
    /// If it has not been set, it will be `crate::constant::DEFAULT_STACK_SIZE`.
    pub fn set_stack_size(&self, stack_size: usize) {
        self.stack_size.store(stack_size, Ordering::Release);
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
        f: impl FnOnce(&Suspender<(), ()>, ()) -> Option<usize> + UnwindSafe + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<JoinHandle<'s>> {
        let coroutine = SchedulableCoroutine::new(
            format!("{}|co-{}", self.get_name(), Uuid::new_v4()),
            f,
            stack_size.unwrap_or(self.get_stack_size()),
        )?;
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.submit_raw_co(coroutine)?;
        Ok(JoinHandle::new(self, co_name))
    }

    /// Submit a closure to create new coroutine, then the coroutine will be push into ready queue.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    pub fn submit_raw_co(
        &self,
        mut coroutine: SchedulableCoroutine<'static>,
    ) -> std::io::Result<()> {
        for listener in &self.listeners {
            #[allow(clippy::transmute_ptr_to_ptr, clippy::missing_transmute_annotations)]
            coroutine.add_raw_listener(unsafe { std::mem::transmute(*listener) });
        }
        coroutine.ready()?;
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
    pub fn try_resume(&self, co_name: &str) -> std::io::Result<()> {
        let co_name: &str = Box::leak(Box::from(co_name));
        unsafe {
            if let Some(co) = SYSTEM_CALL_TABLE.remove(co_name) {
                let state = co.state();
                match state {
                    CoroutineState::SystemCall(val, syscall, SyscallState::Suspend(_)) => {
                        co.syscall(val, syscall, SyscallState::Callback)
                            .expect("change syscall state failed");
                    }
                    _ => panic!("try_resume should never execute to here {co_name} {state}"),
                }
                self.ready.push_back(co);
            }
        }
        Ok(())
    }

    /// Schedule the coroutines.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_schedule(&self) -> std::io::Result<()> {
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
    pub fn try_timed_schedule(&self, dur: std::time::Duration) -> std::io::Result<u64> {
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
    pub fn try_timeout_schedule(&self, timeout_time: u64) -> std::io::Result<u64> {
        if self
            .scheduling
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(timeout_time.saturating_sub(open_coroutine_timer::now()));
        }
        Self::init_current(self);
        let left_time = self.do_schedule(timeout_time);
        Self::clean_current();
        self.scheduling.store(false, Ordering::Release);
        left_time
    }

    fn do_schedule(&self, timeout_time: u64) -> std::io::Result<u64> {
        loop {
            let left_time = timeout_time.saturating_sub(open_coroutine_timer::now());
            if left_time == 0 {
                return Ok(0);
            }
            self.check_ready()?;
            // schedule coroutines
            if let Some(mut coroutine) = self.ready.pop_front() {
                match coroutine.resume() {
                    Ok(state) => match state {
                        CoroutineState::SystemCall((), _, state) => {
                            //挂起协程到系统调用表
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            //如果已包含，说明当前系统调用还有上层父系统调用，因此直接忽略插入结果
                            unsafe {
                                _ = SYSTEM_CALL_TABLE.insert(co_name, coroutine);
                                if let SyscallState::Suspend(timestamp) = state {
                                    SYSTEM_CALL_SUSPEND_TABLE.insert(timestamp, co_name);
                                }
                            }
                        }
                        CoroutineState::Suspend((), timestamp) => {
                            if timestamp > open_coroutine_timer::now() {
                                //挂起协程到时间轮
                                unsafe { SUSPEND_TABLE.insert(timestamp, coroutine) };
                            } else {
                                //放入就绪队列尾部
                                self.ready.push_back(coroutine);
                            }
                        }
                        CoroutineState::Complete(result) => {
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            assert!(
                                self.results.insert(co_name, Ok(result)).is_none(),
                                "not consume result"
                            );
                        }
                        CoroutineState::Error(message) => {
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            assert!(
                                self.results.insert(co_name, Err(message)).is_none(),
                                "not consume result"
                            );
                        }
                        _ => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "try_timeout_schedule should never execute to here",
                            ));
                        }
                    },
                    Err(e) => {
                        error!("{} resume failed: {:?}", coroutine.get_name(), e);
                        return Err(e);
                    }
                };
            } else {
                return Ok(left_time);
            }
        }
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
            // Check if the elements in the syscall suspend queue are ready
            for _ in 0..SYSTEM_CALL_SUSPEND_TABLE.entry_len() {
                if let Some((exec_time, _)) = SYSTEM_CALL_SUSPEND_TABLE.front() {
                    if open_coroutine_timer::now() < *exec_time {
                        break;
                    }
                    if let Some((_, mut entry)) = SYSTEM_CALL_SUSPEND_TABLE.pop_front() {
                        while let Some(co_name) = entry.pop_front() {
                            if let Some(coroutine) = SYSTEM_CALL_TABLE.remove(&co_name) {
                                match coroutine.state() {
                                    CoroutineState::SystemCall(val, syscall, state) => {
                                        if let SyscallState::Suspend(_) = state {
                                            coroutine.syscall(
                                                val,
                                                syscall,
                                                SyscallState::Timeout,
                                            )?;
                                        }
                                        self.ready.push_back(coroutine);
                                    }
                                    _ => {
                                        unreachable!("check_ready should never execute to here")
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Attempt to obtain coroutine result with the given `co_name`.
    pub fn try_get_co_result(&self, co_name: &str) -> Option<Result<Option<usize>, &'s str>> {
        self.results.remove(co_name).map(|r| r.1)
    }

    /// Returns `true` if the ready queue, suspend queue, and syscall queue are all empty.
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Returns the number of coroutines owned by this scheduler.
    pub fn size(&self) -> usize {
        self.ready.len() + unsafe { SUSPEND_TABLE.len() + SYSTEM_CALL_TABLE.len() }
    }

    /// Add a listener to this scheduler.
    pub fn add_listener(&mut self, listener: impl SchedulableListener + 's) {
        self.listeners.push_back(Box::leak(Box::new(listener)));
    }
}

impl Default for Scheduler<'_> {
    fn default() -> Self {
        Self::new(
            format!("open-coroutine-scheduler-{:?}", std::thread::current().id()),
            crate::constants::DEFAULT_STACK_SIZE,
        )
    }
}

impl Drop for Scheduler<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.ready.is_empty(),
                "There are still coroutines to be carried out in the ready queue:{:#?} !",
                self.ready
            );
        }
    }
}

impl Named for Scheduler<'_> {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl_for_named!(Scheduler<'s>);

impl_current_for!(SCHEDULER, Scheduler<'s>);

impl<'s, DerefScheduler: Deref<Target = Scheduler<'s>>> Named for DerefScheduler {
    fn get_name(&self) -> &str {
        Box::leak(Box::from(self.deref().get_name()))
    }
}
