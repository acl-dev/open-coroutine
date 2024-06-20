#[cfg(feature = "log")]
use crate::common::Named;
use crate::constants::CoroutineState;
use crate::coroutine::Coroutine;
use std::fmt::Debug;
use std::panic::UnwindSafe;

/// A trait mainly used for monitors.
#[allow(unused_variables)]
pub trait Listener<Param, Yield, Return>: Debug
where
    Param: UnwindSafe,
    Yield: Debug + Copy + UnwindSafe,
    Return: Debug + Copy + UnwindSafe,
{
    /// Callback when the coroutine is created.
    fn on_create(&self, co: &Coroutine<Param, Yield, Return>, stack_size: usize) {}

    /// Callback after changing the status of coroutine.
    fn on_state_changed(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// Callback after changing the coroutine status to ready.
    fn on_ready(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// Callback after changing the coroutine status to running.
    fn on_running(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// Callback after changing the coroutine status to suspend.
    fn on_suspend(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// callback when the coroutine enters syscall.
    fn on_syscall(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
    }

    /// Callback when the coroutine is completed.
    fn on_complete(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        result: Return,
    ) {
    }

    /// Callback when the coroutine is completed with errors, usually, panic occurs.
    fn on_error(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        message: &str,
    ) {
    }
}

macro_rules! invoke_listeners {
    ($self:expr, $method_name:expr, $method:ident($( $args:expr ),*)) => {
        for listener in &$self.listeners {
            _ = $crate::catch!(|| listener.$method( $( $args ),* ),
                "Listener failed without message",
                ($( $args ),*).0.get_name(),
                $method_name
            );
        }
    }
}

impl<Param, Yield, Return> Listener<Param, Yield, Return> for Coroutine<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Debug + Copy + UnwindSafe,
    Return: Debug + Copy + UnwindSafe,
{
    fn on_create(&self, co: &Coroutine<Param, Yield, Return>, stack_size: usize) {
        invoke_listeners!(self, "on_create", on_create(co, stack_size));
    }

    fn on_state_changed(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>,
    ) {
        invoke_listeners!(
            self,
            "on_state_changed",
            on_state_changed(co, old_state, new_state)
        );
    }

    fn on_ready(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
        invoke_listeners!(self, "on_ready", on_ready(co, old_state));
    }

    fn on_running(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
        invoke_listeners!(self, "on_running", on_running(co, old_state));
    }

    fn on_suspend(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
        invoke_listeners!(self, "on_suspend", on_suspend(co, old_state));
    }

    fn on_syscall(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
    ) {
        invoke_listeners!(self, "on_syscall", on_syscall(co, old_state));
    }

    fn on_complete(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        result: Return,
    ) {
        invoke_listeners!(self, "on_complete", on_complete(co, old_state, result));
    }

    fn on_error(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        old_state: CoroutineState<Yield, Return>,
        message: &str,
    ) {
        invoke_listeners!(self, "on_error", on_error(co, old_state, message));
    }
}
