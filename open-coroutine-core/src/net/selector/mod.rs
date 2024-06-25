use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use std::time::Duration;

/// Interest abstraction.
pub(crate) trait Interest: Copy {
    /// create a readable interest.
    fn read(token: usize) -> Self;

    /// create a writable interest.
    fn write(token: usize) -> Self;

    /// create an interest both readable and writable.
    fn read_and_write(token: usize) -> Self;
}

/// Event abstraction.
pub(crate) trait Event {
    /// get the token.
    fn get_token(&self) -> usize;

    /// readable or not.
    fn readable(&self) -> bool;

    /// writable or not.
    fn writable(&self) -> bool;
}

static TOKEN_FD: Lazy<DashMap<usize, c_int>> = Lazy::new(DashMap::new);

static READABLE_RECORDS: Lazy<DashSet<c_int>> = Lazy::new(DashSet::new);

static READABLE_TOKEN_RECORDS: Lazy<DashMap<c_int, usize>> = Lazy::new(DashMap::new);

static WRITABLE_RECORDS: Lazy<DashSet<c_int>> = Lazy::new(DashSet::new);

static WRITABLE_TOKEN_RECORDS: Lazy<DashMap<c_int, usize>> = Lazy::new(DashMap::new);

/// Events abstraction.
pub(crate) trait EventIterator<E: Event> {
    /// get the iterator.
    fn iterator<'a>(&'a self) -> impl Iterator<Item = &'a E>
    where
        E: 'a;
}

/// Event driven abstraction.
pub(crate) trait Selector<I: Interest, E: Event, S: EventIterator<E>> {
    /// # Errors
    /// if poll failed.
    fn select(&self, events: &mut S, timeout: Option<Duration>) -> std::io::Result<()> {
        let result = self.do_select(events, timeout);
        for event in events.iterator() {
            let token = event.get_token();
            let fd = TOKEN_FD.remove(&token).map_or(0, |r| r.1);
            if event.readable() {
                _ = READABLE_TOKEN_RECORDS.remove(&fd);
            }
            if event.writable() {
                _ = WRITABLE_TOKEN_RECORDS.remove(&fd);
            }
        }
        result
    }

    /// # Errors
    /// if add failed.
    fn add_read_event(&self, fd: c_int, token: usize) -> std::io::Result<()> {
        if READABLE_RECORDS.contains(&fd) {
            return Ok(());
        }
        if WRITABLE_RECORDS.contains(&fd) {
            //同时对读写事件感兴趣
            let interests = I::read_and_write(token);
            self.reregister(fd, token, interests)
                .or(self.register(fd, token, interests))
        } else {
            self.register(fd, token, I::read(token))
        }?;
        _ = READABLE_RECORDS.insert(fd);
        _ = READABLE_TOKEN_RECORDS.insert(fd, token);
        Ok(())
    }

    /// # Errors
    /// if add failed.
    fn add_write_event(&self, fd: c_int, token: usize) -> std::io::Result<()> {
        if WRITABLE_RECORDS.contains(&fd) {
            return Ok(());
        }
        if READABLE_RECORDS.contains(&fd) {
            //同时对读写事件感兴趣
            let interests = I::read_and_write(token);
            self.reregister(fd, token, interests)
                .or(self.register(fd, token, interests))
        } else {
            self.register(fd, token, I::write(token))
        }?;
        _ = WRITABLE_RECORDS.insert(fd);
        _ = WRITABLE_TOKEN_RECORDS.insert(fd, token);
        Ok(())
    }

    /// # Errors
    /// if delete failed.
    fn del_event(&self, fd: c_int) -> std::io::Result<()> {
        if READABLE_RECORDS.contains(&fd) || WRITABLE_RECORDS.contains(&fd) {
            let token = READABLE_TOKEN_RECORDS
                .remove(&fd)
                .or(WRITABLE_TOKEN_RECORDS.remove(&fd))
                .map_or(0, |r| r.1);
            self.deregister(fd, token)?;
            _ = READABLE_RECORDS.remove(&fd);
            _ = WRITABLE_RECORDS.remove(&fd);
        }
        Ok(())
    }

    /// # Errors
    /// if delete failed.
    ///
    /// # Panics
    /// if clean failed.
    fn del_read_event(&self, fd: c_int) -> std::io::Result<()> {
        if READABLE_RECORDS.contains(&fd) {
            if WRITABLE_RECORDS.contains(&fd) {
                //写事件不能删
                let token = WRITABLE_TOKEN_RECORDS.get(&fd).map_or(0, |r| *r.value());
                self.reregister(fd, token, I::write(token))?;
                assert!(
                    READABLE_RECORDS.remove(&fd).is_some(),
                    "Clean READABLE_RECORDS failed !"
                );
                assert!(
                    READABLE_TOKEN_RECORDS.remove(&fd).is_some(),
                    "Clean READABLE_TOKEN_RECORDS failed !"
                );
            } else {
                self.del_event(fd)?;
            }
        }
        Ok(())
    }

    /// # Errors
    /// if delete failed.
    ///
    /// # Panics
    /// if clean failed.
    fn del_write_event(&self, fd: c_int) -> std::io::Result<()> {
        if WRITABLE_RECORDS.contains(&fd) {
            if READABLE_RECORDS.contains(&fd) {
                //读事件不能删
                let token = READABLE_TOKEN_RECORDS.get(&fd).map_or(0, |r| *r.value());
                self.reregister(fd, token, I::read(token))?;
                assert!(
                    WRITABLE_RECORDS.remove(&fd).is_some(),
                    "Clean WRITABLE_RECORDS failed !"
                );
                assert!(
                    WRITABLE_TOKEN_RECORDS.remove(&fd).is_some(),
                    "Clean WRITABLE_TOKEN_RECORDS failed !"
                );
            } else {
                self.del_event(fd)?;
            }
        }
        Ok(())
    }

    /// For inner use.
    fn register(&self, fd: c_int, token: usize, interests: I) -> std::io::Result<()> {
        self.do_register(fd, token, interests).map(|()| {
            _ = TOKEN_FD.insert(token, fd);
        })
    }

    /// For inner use.
    fn reregister(&self, fd: c_int, token: usize, interests: I) -> std::io::Result<()> {
        self.do_reregister(fd, token, interests).map(|()| {
            _ = TOKEN_FD.insert(token, fd);
        })
    }

    /// For inner use.
    fn deregister(&self, fd: c_int, token: usize) -> std::io::Result<()> {
        self.do_deregister(fd, token).map(|()| {
            _ = TOKEN_FD.remove(&token);
        })
    }

    /// For inner impls.
    fn do_select(&self, events: &mut S, timeout: Option<Duration>) -> std::io::Result<()>;

    /// For inner impls.
    fn do_register(&self, fd: c_int, token: usize, interests: I) -> std::io::Result<()>;

    /// For inner impls.
    fn do_reregister(&self, fd: c_int, token: usize, interests: I) -> std::io::Result<()>;

    /// For inner impls.
    fn do_deregister(&self, fd: c_int, token: usize) -> std::io::Result<()>;
}

#[cfg(unix)]
pub(super) use {mio::Events, mio_adapter::Poller};

#[cfg(unix)]
mod mio_adapter;

#[cfg(windows)]
pub use {polling::Poller, polling_adapter::Events};

#[cfg(windows)]
mod polling_adapter;
