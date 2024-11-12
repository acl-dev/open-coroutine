use crate::impl_display_by_debug;
use std::collections::{BTreeMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A queue for managing multiple entries under a specified timestamp.
#[repr(C)]
#[derive(Debug, Eq, PartialEq)]
pub struct TimerEntry<T> {
    timestamp: u64,
    inner: VecDeque<T>,
}

impl<T> Deref for TimerEntry<T> {
    type Target = VecDeque<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for TimerEntry<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> TimerEntry<T> {
    /// Creates an empty deque.
    #[must_use]
    pub fn new(timestamp: u64) -> Self {
        TimerEntry {
            timestamp,
            inner: VecDeque::new(),
        }
    }

    /// Get the timestamp.
    #[must_use]
    pub fn get_timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Removes and returns the `t` from the deque.
    /// Whichever end is closer to the removal point will be moved to make
    /// room, and all the affected elements will be moved to new positions.
    /// Returns `None` if `t` not found.
    pub fn remove(&mut self, t: &T) -> Option<T>
    where
        T: Ord,
    {
        let index = self
            .inner
            .binary_search_by(|x| x.cmp(t))
            .unwrap_or_else(|x| x);
        self.inner.remove(index)
    }
}

impl_display_by_debug!(TimerEntry<T>);

/// A queue for managing multiple `TimerEntry`.
#[repr(C)]
#[derive(educe::Educe)]
#[educe(Debug, Eq, PartialEq)]
pub struct TimerList<T> {
    inner: BTreeMap<u64, TimerEntry<T>>,
    #[educe(PartialEq(ignore))]
    total: AtomicUsize,
}

impl<T> Default for TimerList<T> {
    fn default() -> Self {
        TimerList {
            inner: BTreeMap::default(),
            total: AtomicUsize::new(0),
        }
    }
}

impl<T> Deref for TimerList<T> {
    type Target = BTreeMap<u64, TimerEntry<T>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for TimerList<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> TimerList<T> {
    /// Returns the number of elements in the deque.
    #[must_use]
    pub fn len(&self) -> usize {
        if self.inner.is_empty() {
            return 0;
        }
        self.total.load(Ordering::Acquire)
    }

    /// Returns the number of entries in the deque.
    #[must_use]
    pub fn entry_len(&self) -> usize {
        self.inner.len()
    }

    /// Inserts an element at `timestamp` within the deque, shifting all elements
    /// with indices greater than or equal to `timestamp` towards the back.
    pub fn insert(&mut self, timestamp: u64, t: T) {
        if let Some(entry) = self.inner.get_mut(&timestamp) {
            entry.push_back(t);
            _ = self.total.fetch_add(1, Ordering::Release);
            return;
        }
        let mut entry = TimerEntry::new(timestamp);
        entry.push_back(t);
        _ = self.total.fetch_add(1, Ordering::Release);
        if let Some(mut entry) = self.inner.insert(timestamp, entry) {
            // concurrent, just retry
            while !entry.is_empty() {
                if let Some(e) = entry.pop_front() {
                    self.insert(timestamp, e);
                }
            }
        }
    }

    /// Provides a reference to the front element, or `None` if the deque is empty.
    #[must_use]
    pub fn front(&self) -> Option<(&u64, &TimerEntry<T>)> {
        self.inner.first_key_value()
    }

    /// Removes the first element and returns it, or `None` if the deque is empty.
    pub fn pop_front(&mut self) -> Option<(u64, TimerEntry<T>)> {
        self.inner.pop_first().map(|(timestamp, entry)| {
            _ = self.total.fetch_sub(entry.len(), Ordering::Release);
            (timestamp, entry)
        })
    }

    /// Returns `true` if the deque is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes and returns the element at `timestamp` from the deque.
    /// Whichever end is closer to the removal point will be moved to make
    /// room, and all the affected elements will be moved to new positions.
    /// Returns `None` if `timestamp` is out of bounds.
    pub fn remove_entry(&mut self, timestamp: &u64) -> Option<TimerEntry<T>> {
        self.inner.remove(timestamp).inspect(|entry| {
            _ = self.total.fetch_sub(entry.len(), Ordering::Release);
        })
    }

    /// Removes and returns the `t` from the deque.
    /// Whichever end is closer to the removal point will be moved to make
    /// room, and all the affected elements will be moved to new positions.
    /// Returns `None` if `t` not found.
    pub fn remove(&mut self, timestamp: &u64, t: &T) -> Option<T>
    where
        T: Ord,
    {
        if let Some(entry) = self.inner.get_mut(timestamp) {
            let val = entry.remove(t).inspect(|_| {
                _ = self.total.fetch_sub(1, Ordering::Release);
            });
            if entry.is_empty() {
                _ = self.remove_entry(timestamp);
            }
            return val;
        }
        None
    }
}

impl_display_by_debug!(TimerList<T>);

#[cfg(test)]
mod tests {
    use crate::common::now;
    use crate::common::timer::TimerList;

    #[test]
    fn test() {
        assert!(now() > 0);
    }

    #[test]
    fn timer_list() {
        let mut list = TimerList::default();
        assert_eq!(list.entry_len(), 0);
        list.insert(1, String::from("data is 1"));
        list.insert(2, String::from("data is 2"));
        list.insert(3, String::from("data is 3"));
        assert_eq!(list.entry_len(), 3);

        let mut entry = list.pop_front().unwrap().1;
        assert_eq!(entry.len(), 1);
        let string = entry.pop_front().unwrap();
        assert_eq!(string, String::from("data is 1"));
        assert_eq!(entry.len(), 0);
    }
}
