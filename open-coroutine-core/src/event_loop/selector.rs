use once_cell::sync::Lazy;
use polling::{Event, Poller};
use std::collections::HashMap;
use std::time::Duration;

static mut TOKEN_FD: Lazy<HashMap<usize, libc::c_int>> = Lazy::new(HashMap::new);

#[derive(Debug)]
pub struct Selector(Poller);

impl Selector {
    pub fn new() -> std::io::Result<Selector> {
        Ok(Selector(Poller::new()?))
    }

    pub fn fd(token: usize) -> libc::c_int {
        unsafe { TOKEN_FD.remove(&token).unwrap_or(0) }
    }

    pub fn select(
        &self,
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> std::io::Result<usize> {
        self.0.wait(events, timeout)
    }

    pub fn register(
        &self,
        source: libc::c_int,
        token: usize,
        interests: Event,
    ) -> std::io::Result<()> {
        self.0.add(source, interests).map(|_| {
            _ = unsafe { TOKEN_FD.insert(token, source) };
        })
    }

    pub fn reregister(
        &self,
        source: libc::c_int,
        token: usize,
        interests: Event,
    ) -> std::io::Result<()> {
        self.0.modify(source, interests).map(|_| {
            _ = unsafe { TOKEN_FD.insert(token, source) };
        })
    }

    pub fn deregister(&self, source: libc::c_int, token: usize) -> std::io::Result<()> {
        self.0.delete(source).map(|_| {
            _ = unsafe { TOKEN_FD.remove(&token) };
        })
    }
}
