use crate::net::event_loop::EventLoop;
use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::time::Duration;

#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(&'static Arc<EventLoop<'static>>, u64);

impl Drop for JoinHandle {
    fn drop(&mut self) {
        if let Ok(task_id) = self.id() {
            self.0.clean_task_result(task_id);
        }
    }
}

impl JoinHandle {
    /// create `JoinHandle` instance.
    pub(crate) fn err(pool: &'static Arc<EventLoop<'static>>) -> Self {
        Self::new(pool, 0)
    }

    /// create `JoinHandle` instance.
    pub(crate) fn new(pool: &'static Arc<EventLoop<'static>>, task_id: u64) -> Self {
        JoinHandle(pool, task_id)
    }

    /// get the task id.
    ///
    /// # Errors
    /// if the task id is invalid.
    pub fn id(&self) -> std::io::Result<u64> {
        if 0 == self.1 {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid task id"));
        }
        Ok(self.1)
    }

    /// join with `Duration`.
    ///
    /// # Errors
    /// see `timeout_at_join`.
    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Result<Option<usize>, &str>> {
        self.timeout_at_join(crate::common::get_timeout_time(dur))
    }

    /// join.
    ///
    /// # Errors
    /// see `timeout_at_join`.
    pub fn join(&self) -> std::io::Result<Result<Option<usize>, &str>> {
        self.timeout_at_join(u64::MAX)
    }

    /// join with timeout.
    ///
    /// # Errors
    /// if join failed.
    pub fn timeout_at_join(
        &self,
        timeout_time: u64,
    ) -> std::io::Result<Result<Option<usize>, &str>> {
        let task_id = self.id()?;
        self.0.wait_task_result(
            task_id,
            Duration::from_nanos(timeout_time.saturating_sub(crate::common::now())),
        )
    }
}
