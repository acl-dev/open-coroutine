use std::collections::vec_deque::{Iter, IterMut};
use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
pub struct TimerEntry<T> {
    time: u64,
    inner: VecDeque<T>,
}

impl<T> TimerEntry<T> {
    pub fn new(time: u64) -> Self {
        TimerEntry {
            time,
            inner: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn get_time(&self) -> u64 {
        self.time
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.inner.pop_front()
    }

    pub fn push_back(&mut self, t: T) {
        self.inner.push_back(t)
    }

    pub fn remove(&mut self, t: T) -> Option<T>
    where
        T: Ord,
    {
        let index = self
            .inner
            .binary_search_by(|x| x.cmp(&t))
            .unwrap_or_else(|x| x);
        self.inner.remove(index)
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.inner.iter_mut()
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.inner.iter()
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct TimerList<T> {
    dequeue: VecDeque<TimerEntry<T>>,
}

impl<T> TimerList<T> {
    pub fn new() -> Self {
        TimerList {
            dequeue: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.dequeue.len()
    }

    pub fn insert(&mut self, time: u64, t: T) {
        let index = self
            .dequeue
            .binary_search_by(|x| x.time.cmp(&time))
            .unwrap_or_else(|x| x);
        match self.dequeue.get_mut(index) {
            Some(entry) => {
                entry.push_back(t);
            }
            None => {
                let mut entry = TimerEntry::new(time);
                entry.push_back(t);
                self.dequeue.insert(index, entry);
            }
        }
    }

    pub fn front(&self) -> Option<&TimerEntry<T>> {
        self.dequeue.front()
    }

    pub fn pop_front(&mut self) -> Option<TimerEntry<T>> {
        self.dequeue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.dequeue.is_empty()
    }

    pub fn get_entry(&mut self, time: u64) -> Option<&mut TimerEntry<T>> {
        let index = self
            .dequeue
            .binary_search_by(|x| x.time.cmp(&time))
            .unwrap_or_else(|x| x);
        self.dequeue.get_mut(index)
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, TimerEntry<T>> {
        self.dequeue.iter_mut()
    }

    pub fn iter(&self) -> Iter<'_, TimerEntry<T>> {
        self.dequeue.iter()
    }
}

impl<T> Default for TimerList<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::TimerList;

    #[test]
    fn timer_list() {
        let mut list = TimerList::new();
        assert_eq!(list.len(), 0);
        list.insert(1, String::from("data can be everything"));
        assert_eq!(list.len(), 1);

        let mut entry = list.pop_front().unwrap();
        assert_eq!(entry.len(), 1);
        let string = entry.pop_front().unwrap();
        assert_eq!(string, String::from("data can be everything"));
    }
}
