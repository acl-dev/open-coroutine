use crate::constants::{Syscall, SyscallState};
use crate::pool::CoroutinePoolImpl;
use crate::scheduler::listener::Listener;
use crate::scheduler::SchedulableCoroutine;

#[derive(Debug)]
pub(crate) struct CoroutineCreator<'p> {
    pool: &'p CoroutinePoolImpl<'p>,
}

impl<'p> CoroutineCreator<'p> {
    pub(crate) fn new(pool: &'p CoroutinePoolImpl<'p>) -> Self {
        CoroutineCreator { pool }
    }
}

impl Listener for CoroutineCreator<'static> {
    fn on_suspend(&self, _co: &SchedulableCoroutine) {
        _ = self.pool.grow();
    }

    fn on_syscall(&self, _co: &SchedulableCoroutine, _: Syscall, _: SyscallState) {
        _ = self.pool.grow();
    }
}
