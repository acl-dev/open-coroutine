use crate::scheduler::SchedulableCoroutine;
use nix::sys::pthread::{pthread_self, Pthread};
use std::ffi::c_void;

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct TaskNode {
    timestamp: u64,
    pthread: Pthread,
    coroutine: *const c_void,
}

impl TaskNode {
    pub fn new(timestamp: u64, coroutine: *const SchedulableCoroutine) -> Self {
        TaskNode {
            timestamp,
            pthread: pthread_self(),
            coroutine: coroutine.cast::<c_void>(),
        }
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn pthread(&self) -> Pthread {
        self.pthread
    }

    pub fn coroutine(&self) -> &SchedulableCoroutine {
        unsafe { &*(self.coroutine.cast::<SchedulableCoroutine>()) }
    }
}
