use crate::common::{Named, Pool, StatePool};
use crate::constants::PoolState;
use crate::pool::task::Task;
use crate::pool::{CoroutinePool, CoroutinePoolImpl, TaskPool, WaitableTaskPool};
use crate::scheduler::has::HasScheduler;
use crate::scheduler::{SchedulableCoroutine, SchedulerImpl};
use std::fmt::Debug;
use std::time::Duration;

#[allow(missing_docs, clippy::missing_errors_doc)]
pub trait HasCoroutinePool<'p> {
    fn pool(&self) -> &CoroutinePoolImpl<'p>;

    fn pool_mut(&mut self) -> &mut CoroutinePoolImpl<'p>;

    fn submit_raw_co(&self, coroutine: SchedulableCoroutine<'static>) -> std::io::Result<()> {
        self.pool().submit_raw_co(coroutine)
    }

    fn submit_raw_task(&self, task: Task<'p>) {
        self.pool().submit_raw_task(task);
    }

    /// pop a task
    fn pop(&self) -> Option<Task<'p>> {
        self.pool().pop()
    }

    fn has_task(&self) -> bool {
        self.pool().has_task()
    }

    fn count(&self) -> usize {
        self.pool().count()
    }

    fn try_get_task_result(
        &self,
        task_name: &str,
    ) -> Option<(String, Result<Option<usize>, &'p str>)> {
        self.pool().try_get_task_result(task_name)
    }

    fn try_schedule_task(&self) -> std::io::Result<()> {
        self.pool().try_schedule_task()
    }

    fn try_timed_schedule_task(&self, dur: Duration) -> std::io::Result<u64> {
        self.pool().try_timed_schedule_task(dur)
    }

    fn try_timeout_schedule_task(&self, timeout_time: u64) -> std::io::Result<u64> {
        self.pool().try_timeout_schedule_task(timeout_time)
    }
}

impl<'p, HasCoroutinePoolImpl: HasCoroutinePool<'p>> HasScheduler<'p> for HasCoroutinePoolImpl {
    fn scheduler(&self) -> &SchedulerImpl<'p> {
        self.pool().scheduler()
    }

    fn scheduler_mut(&mut self) -> &mut SchedulerImpl<'p> {
        self.pool_mut().scheduler_mut()
    }
}

impl<'p, HasCoroutinePoolImpl: HasCoroutinePool<'p> + Debug> Pool for HasCoroutinePoolImpl {
    fn set_min_size(&self, min_size: usize) {
        self.pool().set_min_size(min_size);
    }

    fn get_min_size(&self) -> usize {
        self.pool().get_min_size()
    }

    fn get_running_size(&self) -> usize {
        self.pool().get_running_size()
    }

    fn set_max_size(&self, max_size: usize) {
        self.pool().set_max_size(max_size);
    }

    fn get_max_size(&self) -> usize {
        self.pool().get_max_size()
    }

    fn set_keep_alive_time(&self, keep_alive_time: u64) {
        self.pool().set_keep_alive_time(keep_alive_time);
    }

    fn get_keep_alive_time(&self) -> u64 {
        self.pool().get_keep_alive_time()
    }
}

impl<'p, HasCoroutinePoolImpl: HasCoroutinePool<'p> + Debug + Named> StatePool
    for HasCoroutinePoolImpl
{
    fn state(&self) -> PoolState {
        self.pool().state()
    }

    fn change_state(&self, state: PoolState) -> PoolState {
        self.pool().change_state(state)
    }
}
