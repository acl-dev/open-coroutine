use crate::common::beans::BeanFactory;
use crate::common::constants::{CoroutineState, SyscallState};
use crate::common::ordered_work_steal::{OrderedLocalQueue, OrderedWorkStealQueue};
use crate::common::{get_timeout_time, now};
use crate::coroutine::listener::Listener;
use crate::coroutine::suspender::Suspender;
use crate::coroutine::Coroutine;
use crate::{co, impl_current_for, impl_display_by_debug, impl_for_named};
use dashmap::DashMap;
use std::collections::{BinaryHeap, VecDeque};
use std::ffi::c_longlong;
use std::io::{Error, ErrorKind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A type for Scheduler.
pub type SchedulableCoroutineState = CoroutineState<(), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableCoroutine<'s> = Coroutine<'s, (), (), Option<usize>>;

/// A type for Scheduler.
pub type SchedulableSuspender<'s> = Suspender<'s, (), ()>;

#[repr(C)]
#[derive(Debug)]
struct SuspendItem<'s> {
    timestamp: u64,
    coroutine: SchedulableCoroutine<'s>,
}

impl PartialEq<Self> for SuspendItem<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp.eq(&other.timestamp)
    }
}

impl Eq for SuspendItem<'_> {}

impl PartialOrd<Self> for SuspendItem<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SuspendItem<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap defaults to a large top heap, but we need a small top heap
        other.timestamp.cmp(&self.timestamp)
    }
}

#[repr(C)]
#[derive(Debug)]
struct SyscallSuspendItem<'s> {
    timestamp: u64,
    co_name: &'s str,
}

impl PartialEq<Self> for SyscallSuspendItem<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp.eq(&other.timestamp)
    }
}

impl Eq for SyscallSuspendItem<'_> {}

impl PartialOrd<Self> for SyscallSuspendItem<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SyscallSuspendItem<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap defaults to a large top heap, but we need a small top heap
        other.timestamp.cmp(&self.timestamp)
    }
}

/// The scheduler impls.
#[repr(C)]
#[derive(Debug)]
pub struct Scheduler<'s> {
    name: String,
    stack_size: AtomicUsize,
    listeners: VecDeque<&'s dyn Listener<(), Option<usize>>>,
    ready: OrderedLocalQueue<'s, SchedulableCoroutine<'s>>,
    suspend: BinaryHeap<SuspendItem<'s>>,
    syscall: DashMap<&'s str, SchedulableCoroutine<'s>>,
    syscall_suspend: BinaryHeap<SyscallSuspendItem<'s>>,
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
            ready: BeanFactory::get_or_default::<OrderedWorkStealQueue<SchedulableCoroutine>>(
                crate::common::constants::COROUTINE_GLOBAL_QUEUE_BEAN,
            )
            .local_queue(),
            suspend: BinaryHeap::default(),
            syscall: DashMap::default(),
            syscall_suspend: BinaryHeap::default(),
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
        priority: Option<c_longlong>,
    ) -> std::io::Result<()> {
        self.submit_raw_co(co!(
            Some(format!("{}@{}", self.name(), uuid::Uuid::new_v4())),
            f,
            Some(stack_size.unwrap_or(self.stack_size())),
            priority
        )?)
    }

    /// Add a listener to this scheduler.
    pub fn add_listener(&mut self, listener: impl Listener<(), Option<usize>> + 's) {
        self.listeners.push_back(Box::leak(Box::new(listener)));
    }

    /// Submit a raw coroutine, then the coroutine will be push into ready queue.
    ///
    /// Allow multiple threads to concurrently submit coroutine to the scheduler,
    /// but only allow one thread to execute scheduling.
    pub fn submit_raw_co(&self, mut co: SchedulableCoroutine<'s>) -> std::io::Result<()> {
        for listener in self.listeners.clone() {
            co.add_raw_listener(listener);
        }
        self.ready.push(co);
        Ok(())
    }

    /// Resume a coroutine from the syscall table to the ready queue,
    /// it's generally only required for framework level crates.
    ///
    /// If we can't find the coroutine, nothing happens.
    ///
    /// # Errors
    /// if change to ready fails.
    pub fn try_resume(&self, co_name: &'s str) {
        if let Some((_, co)) = self.syscall.remove(&co_name) {
            match co.state() {
                CoroutineState::Syscall(val, syscall, SyscallState::Suspend(_)) => {
                    co.syscall(val, syscall, SyscallState::Callback)
                        .expect("change syscall state failed");
                }
                _ => unreachable!("try_resume unexpect CoroutineState"),
            }
            self.ready.push(co);
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
            if let Some(mut coroutine) = self.ready.pop() {
                match coroutine.resume()? {
                    CoroutineState::Syscall((), _, state) => {
                        //挂起协程到系统调用表
                        let co_name = Box::leak(Box::from(coroutine.name()));
                        //如果已包含，说明当前系统调用还有上层父系统调用，因此直接忽略插入结果
                        _ = self.syscall.insert(co_name, coroutine);
                        if let SyscallState::Suspend(timestamp) = state {
                            self.syscall_suspend
                                .push(SyscallSuspendItem { timestamp, co_name });
                        }
                    }
                    CoroutineState::Suspend((), timestamp) => {
                        if timestamp > now() {
                            //挂起协程到时间轮
                            self.suspend.push(SuspendItem {
                                timestamp,
                                coroutine,
                            });
                        } else {
                            //放入就绪队列尾部
                            self.ready.push(coroutine);
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
        while let Some(item) = self.suspend.peek() {
            if now() < item.timestamp {
                break;
            }
            if let Some(item) = self.suspend.pop() {
                item.coroutine.ready()?;
                self.ready.push(item.coroutine);
            }
        }
        // Check if the elements in the syscall suspend queue are ready
        while let Some(item) = self.syscall_suspend.peek() {
            if now() < item.timestamp {
                break;
            }
            if let Some(item) = self.syscall_suspend.pop() {
                if let Some((_, co)) = self.syscall.remove(item.co_name) {
                    match co.state() {
                        CoroutineState::Syscall(val, syscall, SyscallState::Suspend(_)) => {
                            co.syscall(val, syscall, SyscallState::Timeout)?;
                            self.ready.push(co);
                        }
                        _ => unreachable!("check_ready should never execute to here"),
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::scheduler::SyscallSuspendItem;
    use std::collections::BinaryHeap;

    #[test]
    fn test_small_heap() {
        let mut heap = BinaryHeap::default();
        for timestamp in (0..10).rev() {
            heap.push(SyscallSuspendItem {
                timestamp,
                co_name: "test",
            });
        }
        for timestamp in 0..10 {
            assert_eq!(timestamp, heap.pop().unwrap().timestamp);
        }
    }
}
