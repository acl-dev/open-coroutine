use object_collection::ObjectList;
use std::collections::vec_deque::{Iter, IterMut};
use std::collections::VecDeque;
use std::ffi::c_void;

#[derive(Debug, PartialEq, Eq)]
pub struct TimerObjectEntry {
    time: u64,
    inner: ObjectList,
}

impl TimerObjectEntry {
    pub fn new(time: u64) -> Self {
        TimerObjectEntry {
            time,
            inner: ObjectList::new(),
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

    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        self.inner.pop_front_raw()
    }

    pub fn push_back<T>(&mut self, t: T) {
        self.inner.push_back(t)
    }

    pub fn remove_raw(&mut self, pointer: *mut c_void) -> Option<*mut c_void> {
        self.inner.remove_raw(pointer)
    }

    pub fn push_back_raw(&mut self, ptr: *mut c_void) {
        self.inner.push_back_raw(ptr)
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, *mut c_void> {
        self.inner.iter_mut()
    }

    pub fn iter(&self) -> Iter<'_, *mut c_void> {
        self.inner.iter()
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct TimerObjectList {
    dequeue: VecDeque<TimerObjectEntry>,
}

impl TimerObjectList {
    pub fn new() -> Self {
        TimerObjectList {
            dequeue: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.dequeue.len()
    }

    pub fn insert<T>(&mut self, time: u64, t: T) {
        let ptr = Box::leak(Box::new(t));
        self.insert_raw(time, ptr as *mut _ as *mut c_void)
    }

    pub fn insert_raw(&mut self, time: u64, ptr: *mut c_void) {
        let index = self
            .dequeue
            .binary_search_by(|x| x.time.cmp(&time))
            .unwrap_or_else(|x| x);
        match self.dequeue.get_mut(index) {
            Some(entry) => {
                entry.push_back_raw(ptr);
            }
            None => {
                let mut entry = TimerObjectEntry::new(time);
                entry.push_back_raw(ptr);
                self.dequeue.insert(index, entry);
            }
        }
    }

    pub fn front(&self) -> Option<&TimerObjectEntry> {
        self.dequeue.front()
    }

    pub fn pop_front(&mut self) -> Option<TimerObjectEntry> {
        self.dequeue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.dequeue.is_empty()
    }

    pub fn get_entry(&mut self, time: u64) -> Option<&mut TimerObjectEntry> {
        let index = self
            .dequeue
            .binary_search_by(|x| x.time.cmp(&time))
            .unwrap_or_else(|x| x);
        self.dequeue.get_mut(index)
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, TimerObjectEntry> {
        self.dequeue.iter_mut()
    }

    pub fn iter(&self) -> Iter<'_, TimerObjectEntry> {
        self.dequeue.iter()
    }
}

impl Default for TimerObjectList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::TimerObjectList;

    #[test]
    fn timer_object_list() {
        let mut list = TimerObjectList::new();
        assert_eq!(list.len(), 0);
        list.insert(1, String::from("data can be everything"));
        assert_eq!(list.len(), 1);

        let mut entry = list.pop_front().unwrap();
        assert_eq!(entry.len(), 1);
        let pointer = entry.pop_front_raw().unwrap() as *mut _ as *mut String;
        unsafe {
            assert_eq!(String::from("data can be everything"), *pointer);
        }
    }
}
