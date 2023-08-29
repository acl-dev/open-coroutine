use crate::coroutine::suspender::Suspender;
use crate::pool::blocker::Blocker;
use crate::pool::creator::CoroutineCreator;
use crate::pool::task::Task;
use crate::scheduler::Scheduler;
use crossbeam_deque::{Injector, Steal};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use uuid::Uuid;

pub mod task;

pub mod blocker;

mod creator;

static RESULT_TABLE: Lazy<DashMap<&str, usize>> = Lazy::new(DashMap::new);

#[derive(Debug)]
pub struct CoroutinePool {
    //任务队列
    task_queue: Injector<Task<'static>>,
    //工作协程组
    workers: Scheduler,
    //协程栈大小
    stack_size: usize,
    //当前协程数
    running: AtomicUsize,
    //当前空闲协程数
    idle: AtomicUsize,
    //最小协程数，即核心协程数
    min_size: usize,
    //最大协程数
    max_size: usize,
    //非核心协程的最大存活时间，单位ns
    keep_alive_time: u64,
    //阻滞器
    blocker: &'static dyn Blocker,
    //是否已初始化协程创建器
    inited: AtomicBool,
}

impl Drop for CoroutinePool {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.is_empty(), "there are still tasks to be carried out !");
        }
    }
}

impl CoroutinePool {
    pub fn new(
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        blocker: impl Blocker + 'static,
    ) -> Self {
        CoroutinePool {
            workers: Scheduler::new(),
            stack_size,
            running: AtomicUsize::new(0),
            idle: AtomicUsize::new(0),
            min_size,
            max_size,
            task_queue: Injector::default(),
            keep_alive_time,
            blocker: Box::leak(Box::new(blocker)),
            inited: AtomicBool::new(false),
        }
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> usize + 'static,
    ) -> &'static str {
        let name: Box<str> = Box::from(Uuid::new_v4().to_string());
        let clone = Box::leak(name.clone());
        self.submit_raw(Task::new(name, f));
        clone
    }

    pub(crate) fn submit_raw(&self, task: Task<'static>) {
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

    fn grow(&'static self) -> std::io::Result<()> {
        if self.task_queue.is_empty() {
            return Ok(());
        }
        if self.running.load(Ordering::Acquire) >= self.max_size {
            return Ok(());
        }
        let create_time = open_coroutine_timer::now();
        _ = self.workers.submit(
            move |suspender, ()| {
                loop {
                    match self.task_queue.steal() {
                        Steal::Empty => {
                            let running = self.running.load(Ordering::Acquire);
                            if open_coroutine_timer::now().saturating_sub(create_time)
                                >= self.keep_alive_time
                                && running > self.min_size
                            {
                                //回收worker协程
                                _ = self.running.fetch_sub(1, Ordering::Release);
                                _ = self.idle.fetch_sub(1, Ordering::Release);
                                return 0;
                            }
                            _ = self.idle.fetch_add(1, Ordering::Release);
                            match self.idle.load(Ordering::Acquire).cmp(&running) {
                                //让出CPU给下一个协程
                                std::cmp::Ordering::Less => suspender.suspend(),
                                //避免CPU在N个无任务的协程中空轮询
                                std::cmp::Ordering::Equal => {
                                    self.blocker.block(Duration::from_millis(1));
                                }
                                std::cmp::Ordering::Greater => {
                                    unreachable!("should never execute to here");
                                }
                            }
                        }
                        Steal::Success(task) => {
                            _ = self.idle.fetch_sub(1, Ordering::Release);
                            let task_name = task.get_name();
                            let result = task.run(suspender);
                            assert!(
                                RESULT_TABLE.insert(task_name, result).is_none(),
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

    pub fn try_timed_schedule(&'static self, time: Duration) -> u64 {
        if self
            .inited
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.workers.add_listener(CoroutineCreator::new(self));
        }
        _ = self.grow();
        self.workers
            .try_timeout_schedule(open_coroutine_timer::get_timeout_time(time))
    }

    //只有框架级crate才需要使用此方法
    pub fn resume_syscall(&self, co_name: &'static str) {
        self.workers.resume_syscall(co_name);
    }

    pub fn get_result(task_name: &'static str) -> Option<usize> {
        RESULT_TABLE.remove(&task_name).map(|r| r.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        #[derive(Debug)]
        struct SleepBlocker {}

        impl Blocker for SleepBlocker {
            fn block(&self, time: Duration) {
                std::thread::sleep(time)
            }
        }

        let pool = Box::leak(Box::new(CoroutinePool::new(0, 0, 2, 0, SleepBlocker {})));
        _ = pool.submit(|_, _| {
            println!("1");
            1
        });
        _ = pool.submit(|_, _| {
            println!("2");
            2
        });
        _ = pool.try_timed_schedule(Duration::from_secs(1));
    }
}
