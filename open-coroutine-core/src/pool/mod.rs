use crate::common::{Blocker, Current, JoinHandle, Named, Pool, StatePool};
use crate::constants::PoolState;
use crate::coroutine::suspender::{SimpleSuspender, Suspender};
use crate::pool::creator::CoroutineCreator;
use crate::pool::task::Task;
use crate::scheduler::has::HasScheduler;
use crate::scheduler::SchedulerImpl;
use crossbeam_deque::{Injector, Steal};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub mod task;

mod current;

mod creator;

#[cfg(test)]
mod tests;

/// The `TaskPool` abstraction.
pub trait TaskPool<'p, Join: JoinHandle<Self>>:
    Debug + Default + RefUnwindSafe + Named + StatePool + HasScheduler<'p>
{
    /// Submit a new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    #[allow(box_pointers)]
    fn submit(
        &self,
        name: Option<String>,
        func: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 'p,
        param: Option<usize>,
    ) -> Join {
        let name = name.unwrap_or(format!("{}|{}", self.get_name(), Uuid::new_v4()));
        self.submit_raw(Task::new(name.clone(), func, param));
        Join::new(self, &name)
    }

    /// Submit new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    fn submit_raw(&self, task: Task<'p>);

    /// pop a task
    fn pop(&self) -> Option<Task<'p>>;

    /// Returns `true` if the task queue is empty.
    fn has_task(&self) -> bool {
        self.count() != 0
    }

    /// Returns the number of tasks owned by this pool.
    fn count(&self) -> usize;

    /// Change the blocker in this pool.
    fn change_blocker(&self, blocker: impl Blocker + 'p) -> Box<dyn Blocker>;
}

/// The `WaitableTaskPool` abstraction.
pub trait WaitableTaskPool<'p, Join: JoinHandle<Self>>: TaskPool<'p, Join> {
    /// Submit a new task to this pool and wait for the task to complete.
    ///
    /// # Errors
    /// see `wait_result`
    #[allow(clippy::type_complexity)]
    fn submit_and_wait(
        &self,
        name: Option<String>,
        func: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 'p,
        param: Option<usize>,
        wait_time: Duration,
    ) -> std::io::Result<Option<(String, Result<Option<usize>, &str>)>> {
        let join = self.submit(name, func, param);
        self.wait_result(join.get_name()?, wait_time)
    }

    /// Attempt to obtain task results with the given `task_name`.
    fn try_get_task_result(&self, task_name: &str)
        -> Option<(String, Result<Option<usize>, &str>)>;

    /// Use the given `task_name` to obtain task results, and if no results are found,
    /// block the current thread for `wait_time`.
    ///
    /// # Errors
    /// if timeout
    #[allow(clippy::type_complexity)]
    fn wait_result(
        &self,
        task_name: &str,
        wait_time: Duration,
    ) -> std::io::Result<Option<(String, Result<Option<usize>, &str>)>>;
}

/// The `AutoConsumableTaskPool` abstraction.
pub trait AutoConsumableTaskPool<'p, Join: JoinHandle<Self>>: WaitableTaskPool<'p, Join> {
    /// Start an additional thread to consume tasks.
    ///
    /// # Errors
    /// if create the additional thread failed.
    fn start(self) -> std::io::Result<Arc<Self>>
    where
        'p: 'static;

    /// Stop this pool.
    ///
    /// # Errors
    /// if timeout.
    fn stop(&self, wait_time: Duration) -> std::io::Result<()>;
}

/// The `CoroutinePool` abstraction.
pub trait CoroutinePool<'p, Join: JoinHandle<Self>>:
    Current<'p> + AutoConsumableTaskPool<'p, Join>
{
    /// Create a new `CoroutinePool` instance.
    fn new(
        name: String,
        cpu: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        blocker: impl Blocker + 'p,
    ) -> Self
    where
        Self: Sized;

    /// Attempt to run a task in current coroutine or thread.
    fn try_run(&self) -> Option<()>;

    /// Create a coroutine in this pool.
    ///
    /// # Errors
    /// if create failed.
    fn grow(&self, should_grow: bool) -> std::io::Result<()>;

    /// Schedule the tasks.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    fn try_schedule_task(&self) -> std::io::Result<()> {
        _ = self.try_timeout_schedule_task(Duration::MAX.as_secs())?;
        Ok(())
    }

    /// Try scheduling the tasks for up to `dur`.
    ///
    /// Allow multiple threads to concurrently submit task to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    fn try_timed_schedule_task(&self, dur: Duration) -> std::io::Result<u64> {
        self.try_timeout_schedule_task(open_coroutine_timer::get_timeout_time(dur))
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
    fn try_timeout_schedule_task(&self, timeout_time: u64) -> std::io::Result<u64>;
}

static RESULT_TABLE: Lazy<DashMap<&str, Result<Option<usize>, &str>>> = Lazy::new(DashMap::new);

#[allow(dead_code)]
#[derive(Debug)]
pub struct CoroutinePoolImpl<'p> {
    //绑定到哪个CPU核心
    cpu: usize,
    //协程池状态
    state: Cell<PoolState>,
    //任务队列
    task_queue: Injector<Task<'p>>,
    //工作协程组
    workers: SchedulerImpl<'p>,
    //协程栈大小
    stack_size: usize,
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
    blocker: RefCell<Box<dyn Blocker + 'p>>,
}

impl RefUnwindSafe for CoroutinePoolImpl<'_> {}

impl Drop for CoroutinePoolImpl<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.is_empty(), "there are still tasks to be carried out !");
        }
    }
}

impl Default for CoroutinePoolImpl<'_> {
    fn default() -> Self {
        Self::new(
            1,
            crate::constants::DEFAULT_STACK_SIZE,
            0,
            65536,
            0,
            crate::common::DelayBlocker::default(),
        )
    }
}

impl<'p> CoroutinePoolImpl<'p> {
    pub fn new(
        cpu: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        blocker: impl Blocker + 'p,
    ) -> Self {
        let mut pool = CoroutinePoolImpl {
            cpu,
            state: Cell::new(PoolState::Created),
            workers: SchedulerImpl::default(),
            stack_size,
            running: AtomicUsize::new(0),
            pop_fail_times: AtomicUsize::new(0),
            min_size: AtomicUsize::new(min_size),
            max_size: AtomicUsize::new(max_size),
            task_queue: Injector::default(),
            keep_alive_time: AtomicU64::new(keep_alive_time),
            blocker: RefCell::new(Box::new(blocker)),
        };
        pool.init();
        pool
    }

    fn init(&mut self) {
        self.add_listener(CoroutineCreator::default());
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 'p,
        param: Option<usize>,
    ) -> &'static str {
        let name = Uuid::new_v4().to_string();
        let clone = name.clone().leak();
        self.submit_raw(Task::new(name, f, param));
        clone
    }

    pub(crate) fn submit_raw(&self, task: Task<'p>) {
        self.task_queue.push(task);
    }

    pub fn pop(&self) -> Option<Task> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }
        loop {
            match self.task_queue.steal() {
                Steal::Success(item) => return Some(item),
                Steal::Retry => continue,
                Steal::Empty => return None,
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.task_queue.is_empty()
    }

    fn grow(&self, should_grow: bool) -> std::io::Result<()> {
        if !should_grow || self.is_empty() || self.get_running_size() >= self.get_max_size() {
            return Ok(());
        }
        let create_time = open_coroutine_timer::now();
        _ = self.submit_co(
            move |suspender, ()| {
                loop {
                    let pool = Self::current().expect("current pool not found");
                    match pool.task_queue.steal() {
                        Steal::Empty => {
                            let running = pool.get_running_size();
                            if open_coroutine_timer::now().saturating_sub(create_time)
                                >= pool.get_keep_alive_time()
                                && running > pool.get_min_size()
                            {
                                //回收worker协程
                                pool.running.store(
                                    pool.get_running_size().saturating_sub(1),
                                    Ordering::Release,
                                );
                                return None;
                            }
                            _ = pool.pop_fail_times.fetch_add(1, Ordering::Release);
                            match pool.pop_fail_times.load(Ordering::Acquire).cmp(&running) {
                                //让出CPU给下一个协程
                                std::cmp::Ordering::Less => suspender.suspend(),
                                //避免CPU在N个无任务的协程中空轮询
                                std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => loop {
                                    if let Ok(blocker) = pool.blocker.try_borrow() {
                                        blocker.block(Duration::from_millis(1));
                                        break;
                                    }
                                    pool.pop_fail_times.store(0, Ordering::Release);
                                },
                            }
                        }
                        Steal::Success(task) => {
                            pool.pop_fail_times.store(0, Ordering::Release);
                            let (task_name, result) = task.run(suspender);
                            assert!(
                                RESULT_TABLE.insert(task_name.leak(), result).is_none(),
                                "The previous result was not retrieved in a timely manner"
                            );
                        }
                        Steal::Retry => continue,
                    }
                }
            },
            if self.stack_size > 0 {
                Some(self.stack_size)
            } else {
                None
            },
        )?;
        _ = self.running.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn try_timed_schedule_task(&self, time: Duration) -> u64 {
        Self::init_current(self);
        let left_time = self.try_timeout_schedule(open_coroutine_timer::get_timeout_time(time));
        Self::clean_current();
        left_time.unwrap()
    }

    pub fn get_result(task_name: &str) -> Option<Result<Option<usize>, &str>> {
        RESULT_TABLE.remove(task_name).map(|r| r.1)
    }
}

impl<'p> HasScheduler<'p> for CoroutinePoolImpl<'p> {
    fn scheduler(&self) -> &SchedulerImpl<'p> {
        &self.workers
    }

    fn scheduler_mut(&mut self) -> &mut SchedulerImpl<'p> {
        &mut self.workers
    }
}

impl Pool for CoroutinePoolImpl<'_> {
    fn set_min_size(&self, min_size: usize) {
        self.min_size.store(min_size, Ordering::Release);
    }

    fn get_min_size(&self) -> usize {
        self.min_size.load(Ordering::Acquire)
    }

    fn get_running_size(&self) -> usize {
        self.running.load(Ordering::Acquire)
    }

    fn set_max_size(&self, max_size: usize) {
        self.max_size.store(max_size, Ordering::Release);
    }

    fn get_max_size(&self) -> usize {
        self.max_size.load(Ordering::Acquire)
    }

    fn set_keep_alive_time(&self, keep_alive_time: u64) {
        self.keep_alive_time
            .store(keep_alive_time, Ordering::Release);
    }

    fn get_keep_alive_time(&self) -> u64 {
        self.keep_alive_time.load(Ordering::Acquire)
    }
}
