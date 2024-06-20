use crate::common::Current;
use crate::constants::CoroutineState;
use crate::coroutine::listener::Listener;
use crate::coroutine::Coroutine;
use crate::monitor::Monitor;
use open_coroutine_timer::get_timeout_time;
use std::fmt::Debug;
use std::panic::UnwindSafe;
use std::time::Duration;

#[repr(C)]
#[derive(Debug, Default)]
pub(crate) struct MonitorListener {}

const NOTIFY_NODE: &str = "MONITOR_NODE";

impl<Param, Yield, Return> Listener<Param, Yield, Return> for MonitorListener
where
    Param: UnwindSafe,
    Yield: Debug + Copy + UnwindSafe,
    Return: Debug + Copy + UnwindSafe,
{
    fn on_state_changed(
        &self,
        co: &Coroutine<Param, Yield, Return>,
        _: CoroutineState<Yield, Return>,
        new_state: CoroutineState<Yield, Return>,
    ) {
        if Monitor::current().is_some() {
            return;
        }
        match new_state {
            CoroutineState::Created | CoroutineState::Ready => {}
            CoroutineState::Running => {
                let timestamp = get_timeout_time(Duration::from_millis(10));
                if let Ok(node) = Monitor::submit(timestamp) {
                    _ = co.put(NOTIFY_NODE, node);
                }
            }
            CoroutineState::Suspend(_, _)
            | CoroutineState::SystemCall(_, _, _)
            | CoroutineState::Complete(_)
            | CoroutineState::Error(_) => {
                if let Some(node) = co.get(NOTIFY_NODE) {
                    _ = Monitor::remove(node);
                }
            }
        }
    }
}
