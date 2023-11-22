use crate::net::selector::{Events, Selector, SelectorImpl};
use std::ffi::c_int;
use std::fmt::Debug;
use std::time::Duration;

#[allow(missing_docs, clippy::missing_errors_doc)]
pub trait HasSelector {
    fn selector(&self) -> &SelectorImpl;
}

impl<HasSelectorImpl: HasSelector + Debug> Selector for HasSelectorImpl {
    fn select(&self, events: &mut Events, timeout: Option<Duration>) -> std::io::Result<()> {
        self.selector().select(events, timeout)
    }

    fn add_read_event(&self, fd: c_int, token: usize) -> std::io::Result<()> {
        self.selector().add_read_event(fd, token)
    }

    fn add_write_event(&self, fd: c_int, token: usize) -> std::io::Result<()> {
        self.selector().add_write_event(fd, token)
    }

    fn del_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector().del_event(fd)
    }

    fn del_read_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector().del_read_event(fd)
    }

    fn del_write_event(&self, fd: c_int) -> std::io::Result<()> {
        self.selector().del_write_event(fd)
    }
}
