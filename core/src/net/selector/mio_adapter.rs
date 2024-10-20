use crate::common::CondvarBlocker;
use crossbeam_utils::atomic::AtomicCell;
use derivative::Derivative;
use mio::event::Event;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use std::ffi::c_int;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

impl super::Interest for Interest {
    fn read(_: usize) -> Self {
        Interest::READABLE
    }

    fn write(_: usize) -> Self {
        Interest::WRITABLE
    }

    fn read_and_write(_: usize) -> Self {
        Interest::READABLE.add(Interest::WRITABLE)
    }
}

impl super::Event for Event {
    fn get_token(&self) -> usize {
        self.token().0
    }

    fn readable(&self) -> bool {
        self.is_readable()
    }

    fn writable(&self) -> bool {
        self.is_writable()
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
#[derive(Derivative)]
#[derivative(Debug)]
pub(crate) struct Poller {
    waiting: AtomicBool,
    blocker: CondvarBlocker,
    #[derivative(Debug = "ignore")]
    inner: AtomicCell<Poll>,
}

impl Poller {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            waiting: AtomicBool::new(false),
            blocker: CondvarBlocker::default(),
            inner: AtomicCell::new(Poll::new()?),
        })
    }
}

impl Deref for Poller {
    type Target = Poll;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.as_ptr() }
    }
}

impl super::Selector<Interest, Event, Events> for Poller {
    fn waiting(&self) -> &AtomicBool {
        &self.waiting
    }

    fn blocker(&self) -> &CondvarBlocker {
        &self.blocker
    }

    fn do_select(&self, events: &mut Events, timeout: Option<Duration>) -> std::io::Result<()> {
        let inner = unsafe { &mut *self.inner.as_ptr() };
        inner.poll(events, timeout)
    }

    fn do_register(&self, fd: c_int, token: usize, interests: Interest) -> std::io::Result<()> {
        self.registry()
            .register(&mut SourceFd(&fd), Token(token), interests)
    }

    fn do_reregister(&self, fd: c_int, token: usize, interests: Interest) -> std::io::Result<()> {
        self.registry()
            .reregister(&mut SourceFd(&fd), Token(token), interests)
    }

    fn do_deregister(&self, fd: c_int, _: usize) -> std::io::Result<()> {
        self.registry().deregister(&mut SourceFd(&fd))
    }
}
