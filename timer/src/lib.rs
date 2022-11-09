use object_collection::ObjectList;
use std::collections::VecDeque;
use std::os::raw::c_void;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NANOS_PER_SEC: u64 = 1_000_000_000;

// get the current wall clock in ns
#[inline]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
        .as_nanos() as u64
}

#[inline]
pub fn dur_to_ns(dur: Duration) -> u64 {
    // Note that a duration is a (u64, u32) (seconds, nanoseconds) pair
    dur.as_secs()
        .saturating_mul(NANOS_PER_SEC)
        .saturating_add(u64::from(dur.subsec_nanos()))
}

pub fn get_timeout_time(dur: Duration) -> u64 {
    let now = now();
    match now.checked_add(dur_to_ns(dur)) {
        Some(time) => time,
        //处理溢出
        None => u64::MAX,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TimerEntry {
    time: u64,
    dequeue: ObjectList,
}

impl TimerEntry {
    pub fn new(time: u64) -> Self {
        TimerEntry {
            time,
            dequeue: ObjectList::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.dequeue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dequeue.is_empty()
    }

    pub fn get_time(&self) -> u64 {
        self.time
    }

    pub fn pop_front<T>(&mut self) -> Option<T> {
        self.dequeue.pop_front()
    }

    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        self.dequeue.pop_front_raw()
    }

    pub fn push_back<T>(&mut self, t: T) {
        self.dequeue.push_back(t)
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct TimerList {
    dequeue: VecDeque<TimerEntry>,
}

impl TimerList {
    pub fn new() -> Self {
        TimerList {
            dequeue: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.dequeue.len()
    }

    pub fn insert<T>(&mut self, time: u64, t: T) {
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

    pub fn front(&self) -> Option<&TimerEntry> {
        self.dequeue.front()
    }

    pub fn pop_front(&mut self) -> Option<TimerEntry> {
        self.dequeue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.dequeue.is_empty()
    }
}

impl Default for TimerList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{now, TimerList};

    #[test]
    fn test() {
        println!("{}", now());
    }

    #[test]
    fn timer_list() {
        let mut list = TimerList::new();
        assert_eq!(list.len(), 0);
        list.insert(1, String::from("data can be everything"));
        assert_eq!(list.len(), 1);

        let mut entry = list.pop_front().unwrap();
        assert_eq!(entry.len(), 1);
        assert!(entry.pop_front::<i32>().is_some());
    }

    #[test]
    fn overflow_or_not() {
        let base = u64::MAX - 1;
        match base.checked_add(1) {
            Some(val) => {
                assert_eq!(u64::MAX, val)
            }
            None => panic!(),
        }
        match base.checked_add(2) {
            Some(_) => {
                panic!()
            }
            None => {
                println!("overflow")
            }
        }
    }
}
