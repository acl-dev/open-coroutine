use crate::net::event_loop::EventLoop;
use std::ffi::{c_char, CStr, CString};
use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::time::Duration;

#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(&'static Arc<EventLoop<'static>>, *const c_char);

impl JoinHandle {
    /// create `JoinHandle` instance.
    pub(crate) fn err(pool: &'static Arc<EventLoop<'static>>) -> Self {
        Self::new(pool, "")
    }

    /// create `JoinHandle` instance.
    pub(crate) fn new(pool: &'static Arc<EventLoop<'static>>, name: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(name).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandle(pool, cstr.as_ptr())
    }

    /// get the task name.
    ///
    /// # Errors
    /// if the task name is invalid.
    pub fn get_name(&self) -> std::io::Result<&str> {
        unsafe { CStr::from_ptr(self.1) }
            .to_str()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid task name"))
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
        let name = self.get_name()?;
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid task name"));
        }
        self.0.wait_task_result(
            name,
            Duration::from_nanos(timeout_time.saturating_sub(crate::common::now())),
        )
    }
}
