use crate::constants::{Syscall, SyscallState};
use crate::pool::CoroutinePool;
use crate::scheduler::listener::Listener;
use crate::scheduler::SchedulableCoroutine;

#[derive(Debug)]
pub(crate) struct CoroutineCreator<'p> {
    pool: &'p CoroutinePool,
}

impl<'p> CoroutineCreator<'p> {
    pub(crate) fn new(pool: &'p CoroutinePool) -> Self {
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
