use crate::common::{Current, JoinHandle, Named};
use crate::coroutine::suspender::Suspender;
use crate::scheduler::join::JoinHandleImpl;
use crate::scheduler::listener::Listener;
use crate::scheduler::{Scheduler, SchedulerImpl};
use std::fmt::Debug;
use std::panic::UnwindSafe;

#[allow(missing_docs, clippy::missing_errors_doc)]
pub trait HasScheduler: Debug + Default {
    fn scheduler<'s>(&self) -> &SchedulerImpl<'s>;

    fn scheduler_mut<'s>(&mut self) -> &mut SchedulerImpl<'s>;
}

impl<HasSchedulerImpl: HasScheduler> Named for HasSchedulerImpl {
    fn get_name(&self) -> &str {
        self.scheduler().get_name()
    }
}

impl<HasSchedulerImpl: HasScheduler> Listener for HasSchedulerImpl {}

impl<'s, HasSchedulerImpl: HasScheduler + Current<'s>> Scheduler<'s, JoinHandleImpl<'s>>
    for HasSchedulerImpl
where
    JoinHandleImpl<'s>: JoinHandle<HasSchedulerImpl>,
{
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

    fn try_get_co_result(&self, co_name: &str) -> Option<Result<Option<usize>, &str>> {
        self.scheduler().try_get_co_result(co_name)
    }

    fn size(&self) -> usize {
        self.scheduler().size()
    }

    fn add_listener(&mut self, listener: impl Listener + 's) {
        self.scheduler_mut().add_listener(listener);
    }
}
