use crate::common::{Blocker, Named};
use crate::net::event_loop::core::EventLoop;
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub(crate) struct SelectBlocker {
    event_loop: &'static EventLoop,
}

impl SelectBlocker {
    pub(crate) fn new(event_loop: &mut EventLoop) -> Self {
        SelectBlocker {
            event_loop: unsafe { Box::leak(Box::from_raw(event_loop)) },
        }
    }
}

impl Named for SelectBlocker {
    fn get_name(&self) -> &str {
        "SelectBlocker"
    }
}

impl Blocker for SelectBlocker {
    fn block(&self, time: Duration) {
        _ = self.event_loop.wait_event(Some(time));
    }
}
