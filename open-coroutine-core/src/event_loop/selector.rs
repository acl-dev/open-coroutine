use once_cell::sync::Lazy;
use polling::{Event, PollMode, Poller};
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

    fn get_mode(&self) -> PollMode {
        if self.0.supports_edge() {
            PollMode::Edge
        } else {
            PollMode::Level
        }
    }

    pub fn register(&self, fd: libc::c_int, token: usize, interests: Event) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0
            .add_with_mode(source, interests, self.get_mode())
            .map(|_| {
                _ = unsafe { TOKEN_FD.insert(token, fd) };
            })
    }

    pub fn reregister(
        &self,
        fd: libc::c_int,
        token: usize,
        interests: Event,
    ) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0
            .modify_with_mode(source, interests, self.get_mode())
            .map(|_| {
                _ = unsafe { TOKEN_FD.insert(token, fd) };
            })
    }

    pub fn deregister(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0.delete(source).map(|_| {
            _ = unsafe { TOKEN_FD.remove(&token) };
        })
    }
}
