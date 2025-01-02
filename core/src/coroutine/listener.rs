use crate::common::constants::CoroutineState;
use crate::coroutine::local::CoroutineLocal;
use crate::coroutine::Coroutine;
use std::fmt::Debug;

/// A trait mainly used for monitors.
#[allow(unused_variables)]
pub trait Listener<Yield, Return>: Debug {
    /// Callback after changing the status of coroutine.
    fn on_state_changed(
        &self,
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// Callback after changing the coroutine status to ready.
    fn on_ready(&self, local: &CoroutineLocal, old_state: CoroutineState<Yield, Return>) {}

    /// Callback after changing the coroutine status to running.
    fn on_running(&self, local: &CoroutineLocal, old_state: CoroutineState<Yield, Return>) {}

    /// Callback after changing the coroutine status to suspend.
    fn on_suspend(&self, local: &CoroutineLocal, old_state: CoroutineState<Yield, Return>) {}

    /// callback when the coroutine enters syscall.
    fn on_syscall(&self, local: &CoroutineLocal, old_state: CoroutineState<Yield, Return>) {}

    /// Callback when the coroutine is completed.
    fn on_complete(
        &self,
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        result: Return,
    ) {
    }

    /// Callback when the coroutine is completed with errors, usually, panic occurs.
    fn on_error(
        &self,
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        message: &str,
    ) {
    }
}

macro_rules! broadcast {
    ($impl_method_name: ident($($arg: ident : $arg_type: ty),*), $method_name:expr) => {
        fn $impl_method_name(&self, $($arg: $arg_type),*) {
            for listener in &self.listeners {
                _ = $crate::catch!(
                    || listener.$impl_method_name($($arg, )*),
                    format!("Listener {} failed without message", $method_name),
                    format!("{} invoke {}", self.name(), $method_name)
                );
            }
        }
    }
}

impl<Param, Yield, Return> Listener<Yield, Return> for Coroutine<'_, Param, Yield, Return>
where
    Yield: Debug + Copy,
    Return: Debug + Copy,
{
    broadcast!(on_state_changed(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>
    ), "on_state_changed");

    broadcast!(on_ready(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>
    ), "on_ready");

    broadcast!(on_running(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>
    ), "on_running");

    broadcast!(on_suspend(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>
    ), "on_suspend");

    broadcast!(on_syscall(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>
    ), "on_syscall");

    broadcast!(on_complete(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        result: Return
    ), "on_complete");

    broadcast!(on_error(
        local: &CoroutineLocal,
        old_state: CoroutineState<Yield, Return>,
        message: &str
    ), "on_error");
}
