use std::os::raw::c_int;
use crate::libfiber::{acl_fiber_check_timer, acl_fiber_schedule_stop, acl_fiber_schedule_with, acl_fiber_scheduled, size_t};

pub enum EventMode {
    Kernel,
    Pool,
    Select,
    WinMsg,
}

impl EventMode {
    fn value(&self) -> c_int {
        match self {
            EventMode::Kernel => 0,
            EventMode::Pool => 1,
            EventMode::Select => 2,
            EventMode::WinMsg => 3,
        }
    }
}

pub struct Scheduler {
    mode: EventMode,
}

impl Scheduler {
    pub fn new(mode: EventMode) -> Self {
        Scheduler { mode }
    }

    pub fn set_mode(&mut self, mode: EventMode) {
        self.mode = mode;
    }

    pub fn start(&self) {
        unsafe {
            acl_fiber_schedule_with(self.mode.value());
        }
    }

    pub fn is_scheduling(&self) -> bool {
        unsafe {
            acl_fiber_scheduled() > 0
        }
    }

    pub fn stop(&self) {
        unsafe {
            acl_fiber_schedule_stop();
        }
    }

    pub fn clean(&self, size: size_t) {
        unsafe {
            acl_fiber_check_timer(size);
        }
    }
}