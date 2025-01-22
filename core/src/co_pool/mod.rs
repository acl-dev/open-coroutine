use crate::co_pool::creator::CoroutineCreator;
use crate::co_pool::task::Task;
use crate::common::beans::BeanFactory;
use crate::common::constants::PoolState;
use crate::common::ordered_work_steal::{OrderedLocalQueue, OrderedWorkStealQueue};
use crate::common::{get_timeout_time, now, CondvarBlocker};
use crate::coroutine::suspender::Suspender;
use crate::scheduler::{SchedulableCoroutine, Scheduler};
use crate::{error, impl_current_for, impl_display_by_debug, impl_for_named, trace};
use dashmap::DashMap;
use std::cell::Cell;
use std::ffi::c_longlong;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Task abstraction and impl.
pub mod task;

/// Coroutine pool state abstraction and impl.
mod state;

/// Creator for coroutine pool.
mod creator;

/// The coroutine pool impls.
#[repr(C)]
#[derive(Debug)]
pub struct CoroutinePool<'p> {
    //协程池状态
    state: Cell<PoolState>,
    //任务队列
    #[doc = include_str!("../../docs/en/ordered-work-steal.md")]
    task_queue: OrderedLocalQueue<'p, Task<'p>>,
    //工作协程组
    workers: Scheduler<'p>,
    //当前协程数
    running: AtomicUsize,
    //尝试取出任务失败的次数
    pop_fail_times: AtomicUsize,
    //最小协程数，即核心协程数
    min_size: AtomicUsize,
    //最大协程数
    max_size: AtomicUsize,
    //非核心协程的最大存活时间，单位ns
    keep_alive_time: AtomicU64,
    //阻滞器
    blocker: Arc<CondvarBlocker>,
    //正在等待结果的
    waits: DashMap<&'p str, Arc<(Mutex<bool>, Condvar)>>,
    //任务执行结果
    results: DashMap<String, Result<Option<usize>, &'p str>>,
}

impl Drop for CoroutinePool<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        self.stop(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("Failed to stop coroutine pool {} !", self.name()));
        assert_eq!(
            PoolState::Stopped,
            self.state(),
            "The coroutine pool is not stopped !"
        );
        assert_eq!(
            0,
            self.get_running_size(),
            "There are still tasks in progress !"
        );
        if !self.task_queue.is_empty() {
            error!("Forget some tasks when closing the pool");
        }
    }
}

impl Default for CoroutinePool<'_> {
    fn default() -> Self {
        Self::new(
            format!("open-coroutine-pool-{:?}", std::thread::current().id()),
            crate::common::constants::DEFAULT_STACK_SIZE,
            0,
            65536,
            0,
        )
    }
}

impl<'p> Deref for CoroutinePool<'p> {
    type Target = Scheduler<'p>;

    fn deref(&self) -> &Self::Target {
        &self.workers
    }
}

impl DerefMut for CoroutinePool<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workers
    }
}

impl_for_named!(CoroutinePool<'p>);

impl_current_for!(COROUTINE_POOL, CoroutinePool<'p>);

impl_display_by_debug!(CoroutinePool<'p>);

impl<'p> CoroutinePool<'p> {
    /// Create a new `CoroutinePool` instance.
    #[must_use]
    pub fn new(
        name: String,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
    ) -> Self {
        let mut workers = Scheduler::new(name, stack_size);
        workers.add_listener(CoroutineCreator::default());
        CoroutinePool {
            state: Cell::new(PoolState::Running),
            workers,
            running: AtomicUsize::new(0),
            pop_fail_times: AtomicUsize::new(0),
            min_size: AtomicUsize::new(min_size),
            max_size: AtomicUsize::new(max_size),
            task_queue: BeanFactory::get_or_default::<OrderedWorkStealQueue<Task<'p>>>(
                crate::common::constants::TASK_GLOBAL_QUEUE_BEAN,
            )
            .local_queue(),
            keep_alive_time: AtomicU64::new(keep_alive_time),
            blocker: Arc::default(),
            results: DashMap::new(),
            waits: DashMap::default(),
        }
    }

    /// Set the minimum coroutine number in this pool.
    pub fn set_min_size(&self, min_size: usize) {
        self.min_size.store(min_size, Ordering::Release);
    }

    /// Get the minimum coroutine number in this pool
    pub fn get_min_size(&self) -> usize {
        self.min_size.load(Ordering::Acquire)
    }

    /// Gets the number of coroutines currently running in this pool.
    pub fn get_running_size(&self) -> usize {
        self.running.load(Ordering::Acquire)
    }

    /// Set the maximum coroutine number in this pool.
    pub fn set_max_size(&self, max_size: usize) {
        self.max_size.store(max_size, Ordering::Release);
    }

    /// Get the maximum coroutine number in this pool.
    pub fn get_max_size(&self) -> usize {
        self.max_size.load(Ordering::Acquire)
    }

    /// Set the maximum idle time running in this pool.
    /// `keep_alive_time` has `ns` units.
    pub fn set_keep_alive_time(&self, keep_alive_time: u64) {
        self.keep_alive_time
            .store(keep_alive_time, Ordering::Release);
    }

    /// Get the maximum idle time running in this pool.
    /// Returns in `ns` units.
    pub fn get_keep_alive_time(&self) -> u64 {
        self.keep_alive_time.load(Ordering::Acquire)
    }

    /// Returns `true` if the task queue is empty.
    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }

    /// Returns the number of tasks owned by this pool.
    pub fn size(&self) -> usize {
        self.task_queue.len()
    }

    /// Stop this coroutine pool.
    pub fn stop(&mut self, dur: Duration) -> std::io::Result<()> {
        match self.state() {
            PoolState::Running => {
                assert_eq!(PoolState::Running, self.stopping()?);
                _ = self.try_timed_schedule_task(dur)?;
                assert_eq!(PoolState::Stopping, self.stopped()?);
                Ok(())
            }
            PoolState::Stopping => Err(Error::new(ErrorKind::Other, "should never happens")),
            PoolState::Stopped => Ok(()),
        }
    }

    /// Submit a new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    pub fn submit_task(
        &self,
        name: Option<String>,
        func: impl FnOnce(Option<usize>) -> Option<usize> + 'p,
        param: Option<usize>,
        priority: Option<c_longlong>,
    ) -> std::io::Result<String> {
        match self.state() {
            PoolState::Running => {}
            PoolState::Stopping | PoolState::Stopped => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "The coroutine pool is stopping or stopped !",
                ))
            }
        }
        let name = name.unwrap_or(format!("{}@{}", self.name(), uuid::Uuid::new_v4()));
        self.submit_raw_task(Task::new(name.clone(), func, param, priority));
        Ok(name)
    }

    /// Submit new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    pub(crate) fn submit_raw_task(&self, task: Task<'p>) {
        self.task_queue.push(task);
        self.blocker.notify();
    }

    /// Attempt to obtain task results with the given `task_name`.
    pub fn try_take_task_result(&self, task_name: &str) -> Option<Result<Option<usize>, &'p str>> {
        self.results.remove(task_name).map(|(_, r)| r)
    }

    /// Use the given `task_name` to obtain task results, and if no results are found,
    /// block the current thread for `wait_time`.
    ///
    /// # Errors
    /// if timeout
    pub fn wait_task_result(
        &self,
        task_name: &str,
        wait_time: Duration,
    ) -> std::io::Result<Result<Option<usize>, &str>> {
        let key = Box::leak(Box::from(task_name));
        if let Some(r) = self.try_take_task_result(key) {
            self.notify(key);
            drop(self.waits.remove(key));
            return Ok(r);
        }
        if SchedulableCoroutine::current().is_some() {
            let timeout_time = get_timeout_time(wait_time);
            loop {
                _ = self.try_run();
                if let Some(r) = self.try_take_task_result(key) {
                    return Ok(r);
                }
                if timeout_time.saturating_sub(now()) == 0 {
                    return Err(Error::new(ErrorKind::TimedOut, "wait timeout"));
                }
            }
        }
        let arc = if let Some(arc) = self.waits.get(key) {
            arc.clone()
        } else {
            let arc = Arc::new((Mutex::new(true), Condvar::new()));
            assert!(self.waits.insert(key, arc.clone()).is_none());
            arc
        };
        let (lock, cvar) = &*arc;
        drop(
            cvar.wait_timeout_while(
                lock.lock()
                    .map_err(|e| Error::new(ErrorKind::Other, format!("{e}")))?,
                wait_time,
                |&mut pending| pending,
            )
            .map_err(|e| Error::new(ErrorKind::Other, format!("{e}")))?,
        );
        if let Some(r) = self.try_take_task_result(key) {
            self.notify(key);
            assert!(self.waits.remove(key).is_some());
            return Ok(r);
        }
        Err(Error::new(ErrorKind::TimedOut, "wait timeout"))
    }

    fn can_recycle(&self) -> bool {
        match self.state() {
            PoolState::Running => false,
            PoolState::Stopping | PoolState::Stopped => true,
        }
    }

    /// Try to create a coroutine in this pool.
    ///
    /// # Errors
    /// if create failed.
    fn try_grow(&self) -> std::io::Result<()> {
        if self.task_queue.is_empty() {
            // No task to run
            trace!("The coroutine pool:{} has no task !", self.name());
            return Ok(());
        }
        let create_time = now();
        self.submit_co(
            move |suspender, ()| {
                loop {
                    let pool = Self::current().expect("current pool not found");
                    if pool.try_run().is_some() {
                        pool.reset_pop_fail_times();
                        continue;
                    }
                    let running = pool.get_running_size();
                    if now().saturating_sub(create_time) >= pool.get_keep_alive_time()
                        && running > pool.get_min_size()
                        || pool.can_recycle()
                    {
                        return None;
                    }
                    _ = pool.pop_fail_times.fetch_add(1, Ordering::Release);
                    match pool.pop_fail_times.load(Ordering::Acquire).cmp(&running) {
                        //让出CPU给下一个协程
                        std::cmp::Ordering::Less => suspender.suspend(),
                        //减少CPU在N个无任务的协程中空轮询
                        std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                            pool.blocker.clone().block(Duration::from_millis(1));
                            pool.reset_pop_fail_times();
                        }
                    }
                }
            },
            None,
            None,
        )
    }

    /// Try to create a coroutine in this pool.
    ///
    /// # Errors
    /// if create failed.
    pub fn submit_co(
        &self,
        f: impl FnOnce(&Suspender<(), ()>, ()) -> Option<usize> + 'static,
        stack_size: Option<usize>,
        priority: Option<c_longlong>,
    ) -> std::io::Result<()> {
        if self.get_running_size() >= self.get_max_size() {
            trace!(
                "The coroutine pool:{} has reached its maximum size !",
                self.name()
            );
            return Err(Error::new(
                ErrorKind::Other,
                "The coroutine pool has reached its maximum size !",
            ));
        }
        self.deref().submit_co(f, stack_size, priority).map(|()| {
            _ = self.running.fetch_add(1, Ordering::Release);
        })
    }

    fn reset_pop_fail_times(&self) {
        self.pop_fail_times.store(0, Ordering::Release);
    }

    fn try_run(&self) -> Option<()> {
        self.task_queue.pop().map(|task| {
            let (task_name, result) = task.run();
            assert!(
                self.results.insert(task_name.clone(), result).is_none(),
                "The previous result was not retrieved in a timely manner"
            );
            self.notify(&task_name);
        })
    }

    fn notify(&self, task_name: &str) {
        if let Some(arc) = self.waits.get(task_name) {
            let (lock, cvar) = &**arc;
            let mut pending = lock.lock().expect("notify task failed");
            *pending = false;
            cvar.notify_one();
        }
    }

    /// Schedule the tasks.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_schedule_task(&mut self) -> std::io::Result<()> {
        self.try_timeout_schedule_task(u64::MAX).map(|_| ())
    }

    /// Try scheduling the tasks for up to `dur`.
    ///
    /// Allow multiple threads to concurrently submit task to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_timed_schedule_task(&mut self, dur: Duration) -> std::io::Result<u64> {
        self.try_timeout_schedule_task(get_timeout_time(dur))
    }

    /// Attempt to schedule the tasks before the `timeout_time` timestamp.
    ///
    /// Allow multiple threads to concurrently submit task to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// Returns the left time in ns.
    ///
    /// # Errors
    /// if change to ready fails.
    pub fn try_timeout_schedule_task(&mut self, timeout_time: u64) -> std::io::Result<u64> {
        match self.state() {
            PoolState::Running | PoolState::Stopping => {
                drop(self.try_grow());
            }
            PoolState::Stopped => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "The coroutine pool is stopped !",
                ))
            }
        }
        Self::init_current(self);
        let r = self.try_timeout_schedule(timeout_time);
        Self::clean_current();
        r.map(|(left_time, _)| left_time)
    }
}
