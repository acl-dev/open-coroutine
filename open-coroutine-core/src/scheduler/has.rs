use crate::common::Named;
use crate::coroutine::suspender::Suspender;
use crate::scheduler::join::JoinHandleImpl;
use crate::scheduler::listener::Listener;
use crate::scheduler::{Scheduler, SchedulerImpl};
use std::fmt::Debug;
use std::panic::UnwindSafe;

#[allow(missing_docs, clippy::missing_errors_doc)]
pub trait HasScheduler<'s>: Debug + Default {
    fn scheduler(&self) -> &SchedulerImpl<'s>;

    fn scheduler_mut(&mut self) -> &mut SchedulerImpl<'s>;

    fn set_stack_size(&self, stack_size: usize) {
        self.scheduler().set_stack_size(stack_size);
    }

    fn submit_co(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, ()) -> Option<usize>
            + UnwindSafe
            + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<JoinHandleImpl<'s>> {
        self.scheduler().submit_co(f, stack_size)
    }

    fn try_resume(&self, co_name: &str) -> std::io::Result<()> {
        self.scheduler().try_resume(co_name)
    }

    fn try_timeout_schedule(&self, timeout_time: u64) -> std::io::Result<u64> {
        self.scheduler().try_timeout_schedule(timeout_time)
    }

    fn try_get_co_result(&self, co_name: &str) -> Option<Result<Option<usize>, &'s str>> {
        self.scheduler().try_get_co_result(co_name)
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