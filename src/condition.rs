use std::os::raw::c_int;
use crate::event::Event;
use crate::libfiber::{ACL_FIBER_COND, acl_fiber_cond_create, acl_fiber_cond_free, acl_fiber_cond_signal, acl_fiber_cond_timedwait, acl_fiber_cond_wait};

pub struct Condition {
    condition: *mut ACL_FIBER_COND,
}

impl Condition {
    pub fn new() -> Self {
        unsafe {
            Condition {
                condition: acl_fiber_cond_create(0)
            }
        }
    }

    pub fn delete(&self) {
        unsafe {
            acl_fiber_cond_free(self.condition);
        }
    }

    pub fn wait(&self, event: &Event) {
        unsafe {
            acl_fiber_cond_wait(self.condition, event.event);
        }
    }

    pub fn timed_wait(&self, event: &Event, delay_ms: c_int) {
        unsafe {
            acl_fiber_cond_timedwait(self.condition, event.event, delay_ms);
        }
    }

    pub fn signal(&self) {
        unsafe {
            acl_fiber_cond_signal(self.condition);
        }
    }
}