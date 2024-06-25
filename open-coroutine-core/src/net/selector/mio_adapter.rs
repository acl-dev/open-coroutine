use mio::event::Event;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use std::cell::UnsafeCell;
use std::ffi::c_int;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
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

#[derive(Debug)]
pub(crate) struct Poller {
    waiting: AtomicBool,
    inner: UnsafeCell<Poll>,
}

impl Poller {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            waiting: AtomicBool::new(false),
            inner: UnsafeCell::new(Poll::new()?),
        })
    }
}

impl Deref for Poller {
    type Target = Poll;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner.get() }
    }
}

impl super::Selector<Interest, Event, Events> for Poller {
    fn do_select(&self, events: &mut Events, timeout: Option<Duration>) -> std::io::Result<()> {
        if self
            .waiting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }
        let inner = unsafe { &mut *self.inner.get() };
        let result = inner.poll(events, timeout);
        self.waiting.store(false, Ordering::Release);
        result
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
