use crate::common::Current;
use crate::coroutine::local::HasCoroutineLocal;
use crate::monitor::Monitor;
use crate::scheduler::listener::Listener;
use crate::scheduler::SchedulableCoroutine;
use std::time::Duration;

#[repr(C)]
#[derive(Debug, Default)]
pub(crate) struct MonitorTaskCreator {}

const MONITOR_TIMESTAMP: &str = "MONITOR_TIMESTAMP";

impl Listener for MonitorTaskCreator {
    fn on_resume(&self, timeout_time: u64, coroutine: &SchedulableCoroutine) {
        if Monitor::current().is_some() {
            return;
        }
        let timestamp =
            open_coroutine_timer::get_timeout_time(Duration::from_millis(10)).min(timeout_time);
        _ = coroutine.put(MONITOR_TIMESTAMP, timestamp);
        Monitor::submit(timestamp, coroutine);
    }

    fn on_suspend(&self, _: u64, coroutine: &SchedulableCoroutine) {
        if Monitor::current().is_some() {
            return;
        }
        if let Some(timestamp) = coroutine.get(MONITOR_TIMESTAMP) {
            Monitor::remove(*timestamp, coroutine);
        }
    }

    fn on_complete(&self, _: u64, coroutine: &SchedulableCoroutine, _: Option<usize>) {
        if Monitor::current().is_some() {
            return;
        }
        if let Some(timestamp) = coroutine.get(MONITOR_TIMESTAMP) {
            Monitor::remove(*timestamp, coroutine);
        }
    }

    fn on_error(&self, _: u64, coroutine: &SchedulableCoroutine, _: &str) {
        if Monitor::current().is_some() {
            return;
        }
        if let Some(timestamp) = coroutine.get(MONITOR_TIMESTAMP) {
            Monitor::remove(*timestamp, coroutine);
        }
    }
}
