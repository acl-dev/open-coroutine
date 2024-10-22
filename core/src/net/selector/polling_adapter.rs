use crate::common::CondvarBlocker;
use polling::{Event, PollMode};
use std::ffi::c_int;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub(crate) type Events = Vec<Event>;

impl super::Interest for Event {
    fn read(token: usize) -> Self {
        Event::readable(token)
    }

    fn write(token: usize) -> Self {
        Event::writable(token)
    }

    fn read_and_write(token: usize) -> Self {
        Event::all(token)
    }
}

impl super::Event for Event {
    fn get_token(&self) -> usize {
        self.key
    }

    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }
}

impl super::EventIterator<Event> for Events {
    fn iterator<'a>(&'a self) -> impl Iterator<Item = &'a Event>
    where
        Event: 'a,
    {
        self.iter()
    }
}

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Poller {
    waiting: AtomicBool,
    blocker: CondvarBlocker,
    inner: polling::Poller,
}

impl Poller {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            waiting: AtomicBool::new(false),
            blocker: CondvarBlocker::default(),
            inner: polling::Poller::new()?,
        })
    }
}

impl Deref for Poller {
    type Target = polling::Poller;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Poller {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl super::Selector<Event, Event, Events> for Poller {
    fn waiting(&self) -> &AtomicBool {
        &self.waiting
    }

    fn blocker(&self) -> &CondvarBlocker {
        &self.blocker
    }

    fn do_select(&self, events: &mut Events, timeout: Option<Duration>) -> std::io::Result<()> {
        self.wait(events, timeout).map(|_| ())
    }

    fn do_register(&self, fd: c_int, _: usize, interests: Event) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(u32::try_from(fd).expect("overflow"));
            } else {
                let source = fd;
            }
        }
        self.add_with_mode(
            source,
            interests,
            if self.supports_edge() {
                PollMode::Edge
            } else {
                PollMode::Level
            },
        )
    }

    fn do_reregister(&self, fd: c_int, _: usize, interests: Event) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(u32::try_from(fd).expect("overflow"));
            } else {
                let source = fd;
            }
        }
        self.modify_with_mode(
            source,
            interests,
            if self.supports_edge() {
                PollMode::Edge
            } else {
                PollMode::Level
            },
        )
    }

    fn do_deregister(&self, fd: c_int, _: usize) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(u32::try_from(fd).expect("overflow"));
            } else {
                let source = fd;
            }
        }
        self.delete(source)
    }
}
