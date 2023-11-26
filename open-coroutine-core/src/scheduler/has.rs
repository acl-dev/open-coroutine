use crate::common::Named;
use crate::scheduler::listener::Listener;
use crate::scheduler::{SchedulableCoroutine, Scheduler, SchedulerImpl};

#[allow(missing_docs, clippy::missing_errors_doc)]
pub trait HasScheduler<'s> {
    fn scheduler(&self) -> &SchedulerImpl<'s>;

    fn scheduler_mut(&mut self) -> &mut SchedulerImpl<'s>;

    fn get_stack_size(&self) -> usize {
        self.scheduler().get_stack_size()
    }

    fn set_stack_size(&self, stack_size: usize) {
        self.scheduler().set_stack_size(stack_size);
    }

    fn submit_raw_co(&self, coroutine: SchedulableCoroutine<'static>) -> std::io::Result<()> {
        self.scheduler().submit_raw_co(coroutine)
    }

    fn try_resume(&self, co_name: &str) -> std::io::Result<()> {
        self.scheduler().try_resume(co_name)
    }

    fn try_schedule(&self) -> std::io::Result<()> {
        self.scheduler().try_schedule()
    }

    fn try_timed_schedule(&self, dur: std::time::Duration) -> std::io::Result<u64> {
        self.scheduler().try_timed_schedule(dur)
    }

    fn try_timeout_schedule(&self, timeout_time: u64) -> std::io::Result<u64> {
        self.scheduler().try_timeout_schedule(timeout_time)
    }

    fn try_get_co_result(&self, co_name: &str) -> Option<Result<Option<usize>, &'s str>> {
        self.scheduler().try_get_co_result(co_name)
    }

    fn is_empty(&self) -> bool {
        self.scheduler().is_empty()
    }

    fn size(&self) -> usize {
        self.scheduler().size()
    }

    fn add_listener(&mut self, listener: impl Listener + 's) {
        self.scheduler_mut().add_listener(listener);
    }
}

impl<'s, HasSchedulerImpl: HasScheduler<'s>> Named for HasSchedulerImpl {
    fn get_name(&self) -> &str {
        Box::leak(Box::from(self.scheduler().get_name()))
    }
}
