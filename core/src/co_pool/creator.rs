use crate::co_pool::CoroutinePool;
use crate::common::constants::CoroutineState;
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::scheduler::SchedulableCoroutineState;
use std::sync::atomic::Ordering;

#[repr(C)]
#[derive(Debug, Default)]
pub(crate) struct CoroutineCreator {}

impl Listener<(), Option<usize>> for CoroutineCreator {
    fn on_state_changed(
        &self,
        _: &CoroutineLocal,
        _: SchedulableCoroutineState,
        new_state: SchedulableCoroutineState,
    ) {
        match new_state {
            CoroutineState::Suspend((), _) | CoroutineState::Syscall((), _, _) => {
                if let Some(pool) = CoroutinePool::current() {
                    _ = pool.try_grow();
                }
            }
            CoroutineState::Complete(_) => {
                if let Some(pool) = CoroutinePool::current() {
                    //worker协程正常退出
                    pool.running
                        .store(pool.get_running_size().saturating_sub(1), Ordering::Release);
                }
            }
            CoroutineState::Cancelled | CoroutineState::Error(_) => {
                if let Some(pool) = CoroutinePool::current() {
                    //worker协程异常退出，需要先回收再创建
                    pool.running
                        .store(pool.get_running_size().saturating_sub(1), Ordering::Release);
                    _ = pool.try_grow();
                }
            }
            _ => {}
        }
    }
}
