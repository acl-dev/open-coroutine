use crate::common::{Blocker, Current, JoinHandler, Named, Pool, StatePool};
use crate::constants::PoolState;
use crate::coroutine::suspender::Suspender;
use crate::pool::creator::CoroutineCreator;
use crate::pool::join::JoinHandle;
use crate::pool::task::Task;
use crate::scheduler::{SchedulableSuspender, Scheduler};
use crate::{error, impl_current_for, impl_for_named};
use crossbeam_deque::{Injector, Steal};
use dashmap::DashMap;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use uuid::Uuid;

/// Task abstraction and impl.
pub mod task;

/// Task join abstraction and impl.
pub mod join;

mod creator;

#[cfg(test)]
mod tests;

#[repr(C)]
#[derive(Debug)]
pub struct CoroutinePool<'p> {
    //绑定到哪个CPU核心
    cpu: usize,
    //协程池状态
    state: Cell<PoolState>,
    //任务队列
    task_queue: Injector<Task<'p>>,
    //工作协程组
    workers: Scheduler<'p>,
    //是否正在调度，不允许多线程并行调度
    scheduling: AtomicBool,
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
    //任务执行结果
    results: DashMap<String, Result<Option<usize>, &'p str>>,
    //正在等待结果的
    waits: DashMap<&'p str, Arc<(Mutex<bool>, Condvar)>>,
}

impl<'p> CoroutinePool<'p> {
    /// Create a new `CoroutinePool` instance.
    pub fn new(
        name: String,
        cpu: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        blocker: impl Blocker + 'p,
    ) -> Self {
        let mut workers = Scheduler::new(name, stack_size);
        workers.add_listener(CoroutineCreator::default());
        CoroutinePool {
            cpu,
            state: Cell::new(PoolState::Created),
            workers,
            scheduling: AtomicBool::new(false),
            running: AtomicUsize::new(0),
            pop_fail_times: AtomicUsize::new(0),
            min_size: AtomicUsize::new(min_size),
            max_size: AtomicUsize::new(max_size),
            task_queue: Injector::default(),
            keep_alive_time: AtomicU64::new(keep_alive_time),
            blocker: RefCell::new(Box::new(blocker)),
            results: DashMap::new(),
            waits: DashMap::default(),
        }
    }

    /// Create a coroutine in this pool.
    ///
    /// # Errors
    /// if create failed.
    fn try_grow(&self) -> std::io::Result<()> {
        if !self.has_task() || self.get_running_size() >= self.get_max_size() {
            return Ok(());
        }
        let create_time = open_coroutine_timer::now();
        self.submit_co(
            move |suspender, ()| {
                loop {
                    if let Some(pool) = Self::current() {
                        if pool.try_run(suspender).is_some() {
                            pool.pop_fail_times.store(0, Ordering::Release);
                            continue;
                        }
                        let recycle = match pool.state() {
                            PoolState::Created | PoolState::Running(_) => false,
                            PoolState::Stopping(_) | PoolState::Stopped => true,
                        };
                        let running = pool.get_running_size();
                        if open_coroutine_timer::now().saturating_sub(create_time)
                            >= pool.get_keep_alive_time()
                            && running > pool.get_min_size()
                            || recycle
                        {
                            return None;
                        }
                        _ = pool.pop_fail_times.fetch_add(1, Ordering::Release);
                        match pool.pop_fail_times.load(Ordering::Acquire).cmp(&running) {
                            //让出CPU给下一个协程
                            std::cmp::Ordering::Less => suspender.suspend(),
                            //减少CPU在N个无任务的协程中空轮询
                            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                                loop {
                                    if let Ok(blocker) = pool.blocker.try_borrow() {
                                        blocker.block(Duration::from_millis(1));
                                        break;
                                    }
                                }
                                pool.pop_fail_times.store(0, Ordering::Release);
                            }
                        }
                    } else {
                        error!("current pool not found");
                        return None;
                    }
                }
            },
            None,
        )
        .map(|_| {
            _ = self.running.fetch_add(1, Ordering::Release);
        })
    }

    /// Attempt to run a task in current coroutine or thread.
    fn try_run(&self, suspender: &Suspender<(), ()>) -> Option<()> {
        self.pop().map(|task| {
            let (task_name, result) = task.run(suspender);
            assert!(
                self.results.insert(task_name.clone(), result).is_none(),
                "The previous result was not retrieved in a timely manner"
            );
            if let Some(arc) = self.waits.get(&*task_name) {
                let (lock, cvar) = &**arc;
                let mut pending = lock.lock().unwrap();
                *pending = false;
                // Notify the condvar that the value has changed.
                cvar.notify_one();
            }
        })
    }

    /// pop a task
    pub(crate) fn pop(&self) -> Option<Task<'p>> {
        // Fast path, if len == 0, then there are no values
        if !self.has_task() {
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

    /// Returns `true` if the task queue is empty.
    pub fn has_task(&self) -> bool {
        self.count() != 0
    }

    /// Returns the number of tasks owned by this pool.
    pub fn count(&self) -> usize {
        self.task_queue.len()
    }

    /// Change the blocker in this pool.
    pub fn change_blocker(&self, blocker: impl Blocker + 'p) -> Box<dyn Blocker + 'p> {
        self.blocker.replace(Box::new(blocker))
    }

    /// Submit a new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    pub fn submit(
        &self,
        name: Option<String>,
        func: impl FnOnce(&Suspender<(), ()>, Option<usize>) -> Option<usize> + UnwindSafe + 'p,
        param: Option<usize>,
    ) -> JoinHandle<'p> {
        let name = name.unwrap_or(format!("{}|task-{}", self.get_name(), Uuid::new_v4()));
        self.submit_raw_task(Task::new(name.clone(), func, param));
        JoinHandle::new(self, &name)
    }

    /// Submit new task to this pool.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    pub fn submit_raw_task(&self, task: Task<'p>) {
        self.task_queue.push(task);
    }

    /// Schedule the tasks.
    ///
    /// Allow multiple threads to concurrently submit task to the pool,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_schedule_task(&self) -> std::io::Result<()> {
        self.try_timeout_schedule_task(Duration::MAX.as_secs())
            .map(|_| ())
    }

    /// Try scheduling the tasks for up to `dur`.
    ///
    /// Allow multiple threads to concurrently submit task to the scheduler,
    /// but only allow one thread to execute scheduling.
    ///
    /// # Errors
    /// see `try_timeout_schedule`.
    pub fn try_timed_schedule_task(&self, dur: Duration) -> std::io::Result<u64> {
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
    pub fn try_timeout_schedule_task(&self, timeout_time: u64) -> std::io::Result<u64> {
        if self
            .scheduling
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(timeout_time.saturating_sub(open_coroutine_timer::now()));
        }
        self.running(true)?;
        Self::init_current(self);
        match self.state() {
            PoolState::Created | PoolState::Running(_) | PoolState::Stopping(_) => {
                self.try_grow()?;
            }
            PoolState::Stopped => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "The coroutine pool is stopped !",
                ))
            }
        };
        let result = self.try_timeout_schedule(timeout_time);
        Self::clean_current();
        self.scheduling.store(false, Ordering::Release);
        result
    }

    /// Submit a new task to this pool and wait for the task to complete.
    ///
    /// # Errors
    /// see `wait_result`
    pub fn submit_and_wait(
        &self,
        name: Option<String>,
        func: impl FnOnce(&Suspender<(), ()>, Option<usize>) -> Option<usize> + UnwindSafe + 'p,
        param: Option<usize>,
        wait_time: Duration,
    ) -> std::io::Result<Option<Result<Option<usize>, &str>>> {
        let join = self.submit(name, func, param);
        self.wait_result(join.get_name()?, wait_time)
    }

    /// Attempt to obtain task results with the given `task_name`.
    pub fn try_get_task_result(&self, task_name: &str) -> Option<Result<Option<usize>, &'p str>> {
        self.results.remove(task_name).map(|r| r.1)
    }

    /// Use the given `task_name` to obtain task results, and if no results are found,
    /// block the current thread for `wait_time`.
    ///
    /// # Errors
    /// if timeout
    pub fn wait_result(
        &self,
        task_name: &str,
        wait_time: Duration,
    ) -> std::io::Result<Option<Result<Option<usize>, &str>>> {
        let key = Box::leak(Box::from(task_name));
        if let Some(r) = self.try_get_task_result(key) {
            _ = self.waits.remove(key);
            return Ok(Some(r));
        }
        if let Some(suspender) = SchedulableSuspender::current() {
            let timeout_time = open_coroutine_timer::get_timeout_time(wait_time);
            loop {
                _ = self.try_run(suspender);
                if let Some(r) = self.try_get_task_result(key) {
                    return Ok(Some(r));
                }
                if timeout_time.saturating_sub(open_coroutine_timer::now()) == 0 {
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
        _ = cvar
            .wait_timeout_while(lock.lock().unwrap(), wait_time, |&mut pending| pending)
            .unwrap();
        if let Some(r) = self.try_get_task_result(key) {
            assert!(self.waits.remove(key).is_some());
            return Ok(Some(r));
        }
        Err(Error::new(ErrorKind::TimedOut, "wait timeout"))
    }
}

impl RefUnwindSafe for CoroutinePool<'_> {}

impl Drop for CoroutinePool<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                !self.has_task(),
                "there are still tasks to be carried out !"
            );
        }
    }
}

impl_for_named!(CoroutinePool<'p>);

impl_current_for!(COROUTINE_POOL, CoroutinePool<'p>);

impl Default for CoroutinePool<'_> {
    fn default() -> Self {
        Self::new(
            format!("open-coroutine-pool-{:?}", std::thread::current().id()),
            1,
            crate::constants::DEFAULT_STACK_SIZE,
            0,
            65536,
            0,
            crate::common::DelayBlocker::default(),
        )
    }
}

impl<'p> Deref for CoroutinePool<'p> {
    type Target = Scheduler<'p>;

    fn deref(&self) -> &Self::Target {
        &self.workers
    }
}

impl<'p> DerefMut for CoroutinePool<'p> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workers
    }
}

impl Pool for CoroutinePool<'_> {
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

impl StatePool for CoroutinePool<'_> {
    fn state(&self) -> PoolState {
        self.state.get()
    }

    fn change_state(&self, state: PoolState) -> PoolState {
        self.state.replace(state)
    }
}
