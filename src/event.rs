use crate::libfiber::{ACL_FIBER_EVENT, acl_fiber_event_create, acl_fiber_event_free, acl_fiber_event_notify, acl_fiber_event_trywait, acl_fiber_event_wait};

pub struct Event {
    pub(crate) event: *mut ACL_FIBER_EVENT,
}

impl Event {
    pub fn new() -> Self {
        unsafe {
            Event {
                event: acl_fiber_event_create(0)
            }
        }
    }

    pub fn delete(&self) {
        unsafe {
            acl_fiber_event_free(self.event)
        }
    }

    pub fn wait(&self) {
        unsafe {
            acl_fiber_event_wait(self.event);
        }
    }

    pub fn try_wait(&self) {
        unsafe {
            acl_fiber_event_trywait(self.event);
        }
    }

    pub fn notify(&self) {
        unsafe {
            acl_fiber_event_notify(self.event);
        }
    }
}