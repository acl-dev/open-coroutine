use once_cell::sync::Lazy;
use polling::{Event, PollMode, Poller};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

static mut TOKEN_FD: Lazy<HashMap<usize, libc::c_int>> = Lazy::new(HashMap::new);

#[derive(Debug)]
pub struct Selector(Poller);

static mut READABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut READABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

static mut WRITABLE_RECORDS: Lazy<HashSet<libc::c_int>> = Lazy::new(HashSet::new);

static mut WRITABLE_TOKEN_RECORDS: Lazy<HashMap<libc::c_int, usize>> = Lazy::new(HashMap::new);

impl Selector {
    pub fn new() -> std::io::Result<Selector> {
        Ok(Selector(Poller::new()?))
    }

    pub fn select(
        &self,
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> std::io::Result<usize> {
        let result = self.0.wait(events, timeout);
        for event in events {
            let token = event.key;
            unsafe {
                let fd = TOKEN_FD.remove(&token).unwrap_or(0);
                if event.readable {
                    _ = READABLE_TOKEN_RECORDS.remove(&fd);
                }
                if event.writable {
                    _ = WRITABLE_TOKEN_RECORDS.remove(&fd);
                }
            }
        }
        result
    }

    pub fn add_read_event(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
            if WRITABLE_RECORDS.contains(&fd) {
                //同时对读写事件感兴趣
                let interests = Event::all(token);
                self.reregister(fd, token, interests)
                    .or(self.register(fd, token, interests))
            } else {
                self.register(fd, token, Event::readable(token))
            }?;
            _ = READABLE_RECORDS.insert(fd);
            _ = READABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    pub fn add_write_event(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                return Ok(());
            }
            if READABLE_RECORDS.contains(&fd) {
                //同时对读写事件感兴趣
                let interests = Event::all(token);
                self.reregister(fd, token, interests)
                    .or(self.register(fd, token, interests))
            } else {
                self.register(fd, token, Event::writable(token))
            }?;
            _ = WRITABLE_RECORDS.insert(fd);
            _ = WRITABLE_TOKEN_RECORDS.insert(fd, token);
        }
        Ok(())
    }

    fn register(&self, fd: libc::c_int, token: usize, interests: Event) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0
            .add_with_mode(source, interests, self.get_mode())
            .map(|()| {
                _ = unsafe { TOKEN_FD.insert(token, fd) };
            })
    }

    fn reregister(&self, fd: libc::c_int, token: usize, interests: Event) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0
            .modify_with_mode(source, interests, self.get_mode())
            .map(|()| {
                _ = unsafe { TOKEN_FD.insert(token, fd) };
            })
    }

    fn get_mode(&self) -> PollMode {
        if self.0.supports_edge() {
            PollMode::Edge
        } else {
            PollMode::Level
        }
    }

    pub fn del_event(&self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) || WRITABLE_RECORDS.contains(&fd) {
                let token = READABLE_TOKEN_RECORDS
                    .remove(&fd)
                    .or(WRITABLE_TOKEN_RECORDS.remove(&fd))
                    .unwrap_or(0);
                self.deregister(fd, token)?;
                _ = READABLE_RECORDS.remove(&fd);
                _ = WRITABLE_RECORDS.remove(&fd);
            }
        }
        Ok(())
    }

    pub fn del_read_event(&self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if READABLE_RECORDS.contains(&fd) {
                if WRITABLE_RECORDS.contains(&fd) {
                    //写事件不能删
                    let token = *WRITABLE_TOKEN_RECORDS.get(&fd).unwrap_or(&0);
                    self.reregister(fd, token, Event::writable(token))?;
                    assert!(READABLE_RECORDS.remove(&fd));
                    assert!(READABLE_TOKEN_RECORDS.remove(&fd).is_some());
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
    }

    pub fn del_write_event(&self, fd: libc::c_int) -> std::io::Result<()> {
        unsafe {
            if WRITABLE_RECORDS.contains(&fd) {
                if READABLE_RECORDS.contains(&fd) {
                    //读事件不能删
                    let token = *READABLE_TOKEN_RECORDS.get(&fd).unwrap_or(&0);
                    self.reregister(fd, token, Event::readable(token))?;
                    assert!(WRITABLE_RECORDS.remove(&fd));
                    assert!(WRITABLE_TOKEN_RECORDS.remove(&fd).is_some());
                } else {
                    self.del_event(fd)?;
                }
            }
        }
        Ok(())
    }

    fn deregister(&self, fd: libc::c_int, token: usize) -> std::io::Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
                let source = std::os::windows::io::RawSocket::from(fd as u64);
            } else {
                let source = fd;
            }
        }
        self.0.delete(source).map(|()| {
            _ = unsafe { TOKEN_FD.remove(&token) };
        })
    }
}
