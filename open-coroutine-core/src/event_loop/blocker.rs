use crate::event_loop::core::EventLoop;
use crate::pool::blocker::Blocker;
use std::time::Duration;

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

impl Blocker for SelectBlocker {
    fn block(&self, time: Duration) {
        _ = self.event_loop.wait_just(Some(time));
    }
}
