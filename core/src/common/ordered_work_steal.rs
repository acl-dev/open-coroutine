use crossbeam_deque::{Injector, Steal};
use crossbeam_skiplist::SkipMap;
use rand::Rng;
use st3::fifo::Worker;
use std::collections::VecDeque;
use std::ffi::c_longlong;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// The highest precedence.
pub const HIGHEST_PRECEDENCE: c_longlong = c_longlong::MIN;

/// The lowest precedence.
pub const LOWEST_PRECEDENCE: c_longlong = c_longlong::MAX;

/// The default precedence.
pub const DEFAULT_PRECEDENCE: c_longlong = 0;

/// Ordered trait for user's datastructures.
pub trait Ordered {
    /// Get the priority of the element.
    fn priority(&self) -> Option<c_longlong>;
}

/// Work stealing global queue, shared by multiple threads.
#[repr(C)]
#[derive(Debug)]
pub struct OrderedWorkStealQueue<T: Debug> {
    shared_queue: SkipMap<c_longlong, Injector<T>>,
    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,
    local_capacity: usize,
    local_queues: VecDeque<SkipMap<c_longlong, Worker<T>>>,
    index: AtomicUsize,
}

impl<T: Debug> Drop for OrderedWorkStealQueue<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            for local_queue in &self.local_queues {
                for entry in local_queue {
                    assert!(entry.value().pop().is_none(), "local queue not empty");
                }
            }
            assert!(self.pop().is_none(), "global queue not empty");
        }
    }
}

impl<T: Debug + Ordered> OrderedWorkStealQueue<T> {
    /// Push an element to the global queue.
    pub fn push(&self, item: T) {
        self.push_with_priority(item.priority().unwrap_or(DEFAULT_PRECEDENCE), item);
    }
}

impl<T: Debug> OrderedWorkStealQueue<T> {
    /// Create a new `WorkStealQueue` instance.
    #[must_use]
    pub fn new(local_queues_size: usize, local_capacity: usize) -> Self {
        OrderedWorkStealQueue {
            shared_queue: SkipMap::new(),
            len: AtomicUsize::new(0),
            local_capacity,
            local_queues: (0..local_queues_size).map(|_| SkipMap::new()).collect(),
            index: AtomicUsize::new(0),
        }
    }

    /// Returns `true` if the global queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the size of the global queue.
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    /// Push an element to the global queue.
    pub fn push_with_priority(&self, priority: c_longlong, item: T) {
        self.shared_queue
            .get_or_insert_with(priority, Injector::new)
            .value()
            .push(item);
        //add count
        self.len
            .store(self.len().saturating_add(1), Ordering::Release);
    }

    /// Pop an element from the global queue.
    pub fn pop(&self) -> Option<T> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }
        for entry in &self.shared_queue {
            loop {
                match entry.value().steal() {
                    Steal::Success(item) => {
                        // Decrement the count.
                        self.len
                            .store(self.len().saturating_sub(1), Ordering::Release);
                        return Some(item);
                    }
                    Steal::Retry => {}
                    Steal::Empty => break,
                }
            }
        }
        None
    }

    /// Get a local queue, this method should be called up to `local_queue_size` times.
    ///
    /// # Panics
    /// should never happen
    pub fn local_queue(&self) -> OrderedLocalQueue<'_, T> {
        let mut index = self.index.fetch_add(1, Ordering::Relaxed);
        if index == usize::MAX {
            self.index.store(0, Ordering::Relaxed);
        }
        index %= self.local_queues.len();
        let local = self
            .local_queues
            .get(index)
            .unwrap_or_else(|| panic!("local queue {index} init failed!"));
        OrderedLocalQueue::new(self, local)
    }
}

impl<T: Debug> Default for OrderedWorkStealQueue<T> {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

/// The work stealing local queue, exclusive to thread.
#[repr(C)]
#[derive(Debug)]
pub struct OrderedLocalQueue<'l, T: Debug> {
    /// Used to schedule bookkeeping tasks every so often.
    tick: AtomicU32,
    shared: &'l OrderedWorkStealQueue<T>,
    stealing: AtomicBool,
    queue: &'l SkipMap<c_longlong, Worker<T>>,
    len: AtomicUsize,
}

impl<T: Debug> Drop for OrderedLocalQueue<'_, T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            for entry in self.queue {
                assert!(entry.value().pop().is_none(), "local queue not empty");
            }
        }
    }
}

impl<T: Debug + Ordered> OrderedLocalQueue<'_, T> {
    /// If the queue is full, first push half to global,
    /// then push the item to global.
    pub fn push(&self, item: T) {
        self.push_with_priority(item.priority().unwrap_or(DEFAULT_PRECEDENCE), item);
    }
}

impl<'l, T: Debug> OrderedLocalQueue<'l, T> {
    fn new(
        shared: &'l OrderedWorkStealQueue<T>,
        queue: &'l SkipMap<c_longlong, Worker<T>>,
    ) -> Self {
        OrderedLocalQueue {
            tick: AtomicU32::new(0),
            shared,
            stealing: AtomicBool::new(false),
            queue,
            len: AtomicUsize::new(0),
        }
    }

    /// Returns `true` if the local queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the local queue is full.
    ///
    /// # Examples
    ///
    /// ```
    /// use open_coroutine_core::common::ordered_work_steal::OrderedWorkStealQueue;
    ///
    /// let queue = OrderedWorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// assert!(local.is_empty());
    /// for i in 0..2 {
    ///     local.push_with_priority(i, i);
    /// }
    /// assert!(local.is_full());
    /// assert_eq!(local.pop(), Some(0));
    /// assert_eq!(local.len(), 1);
    /// assert_eq!(local.pop(), Some(1));
    /// assert_eq!(local.pop(), None);
    /// assert!(local.is_empty());
    /// ```
    pub fn is_full(&self) -> bool {
        self.len() >= self.shared.local_capacity
    }

    fn max_steal(&self) -> usize {
        //最多偷取本地最长的一半
        self.shared
            .local_capacity
            .saturating_add(1)
            .saturating_div(2)
            .saturating_sub(self.len())
    }

    fn can_steal(&self) -> bool {
        self.len()
            < self
                .shared
                .local_capacity
                .saturating_add(1)
                .saturating_div(2)
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    fn try_lock(&self) -> bool {
        self.stealing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    fn release_lock(&self) {
        self.stealing.store(false, Ordering::Release);
    }

    /// If the queue is full, first push half to global,
    /// then push the item to global.
    ///
    /// # Examples
    ///
    /// ```
    /// use open_coroutine_core::common::ordered_work_steal::OrderedWorkStealQueue;
    ///
    /// let queue = OrderedWorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     local.push_with_priority(i, i);
    /// }
    /// for i in 0..4 {
    ///     assert_eq!(local.pop(), Some(i));
    /// }
    /// assert_eq!(local.pop(), None);
    /// ```
    pub fn push_with_priority(&self, priority: c_longlong, item: T) {
        if self.is_full() {
            self.push_to_global(priority, item);
            return;
        }
        if let Err(item) = self
            .queue
            .get_or_insert_with(priority, || Worker::new(self.shared.local_capacity))
            .value()
            .push(item)
        {
            self.push_to_global(priority, item);
        } else {
            //add count
            self.len
                .store(self.len().saturating_add(1), Ordering::Release);
        }
    }

    fn push_to_global(&self, priority: c_longlong, item: T) {
        //把本地队列的一半放到全局队列
        let count = self.len() / 2;
        for _ in 0..count {
            for entry in self.queue.iter().rev() {
                if let Some(item) = entry.value().pop() {
                    self.shared.push_with_priority(*entry.key(), item);
                }
            }
        }
        //直接放到全局队列
        self.shared.push_with_priority(priority, item);
    }

    /// Increment the tick
    fn tick(&self) -> u32 {
        let val = self.tick.fetch_add(1, Ordering::Release);
        if val == u32::MAX {
            self.tick.store(0, Ordering::Release);
            return 0;
        }
        val.saturating_add(1)
    }

    /// If the queue is empty, first try steal from global,
    /// then try steal from siblings.
    ///
    /// # Examples
    ///
    /// ```
    /// use open_coroutine_core::common::ordered_work_steal::OrderedWorkStealQueue;
    ///
    /// let queue = OrderedWorkStealQueue::new(1, 32);
    /// for i in 0..4 {
    ///     queue.push_with_priority(i, i);
    /// }
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     assert_eq!(local.pop(), Some(i));
    /// }
    /// assert_eq!(local.pop(), None);
    /// assert_eq!(queue.pop(), None);
    /// ```
    ///
    /// # Examples
    /// ```
    /// use open_coroutine_core::common::ordered_work_steal::OrderedWorkStealQueue;
    ///
    /// let queue = OrderedWorkStealQueue::new(2, 64);
    /// let local0 = queue.local_queue();
    /// for i in 2..6 {
    ///     local0.push_with_priority(i, i);
    /// }
    /// assert_eq!(local0.len(), 4);
    /// let local1 = queue.local_queue();
    /// for i in 0..2 {
    ///     local1.push_with_priority(i, i);
    /// }
    /// assert_eq!(local1.len(), 2);
    /// for i in 0..6 {
    ///     assert_eq!(local1.pop(), Some(i));
    /// }
    /// assert_eq!(local0.pop(), None);
    /// assert_eq!(local1.pop(), None);
    /// assert_eq!(queue.pop(), None);
    /// ```
    pub fn pop(&self) -> Option<T> {
        //每从本地弹出61次，就从全局队列弹出
        if self.tick() % 61 == 0 {
            if let Some(val) = self.shared.pop() {
                return Some(val);
            }
        }
        if let Some(val) = self.pop_local() {
            return Some(val);
        }
        if self.try_lock() {
            //尝试从其他本地队列steal
            let local_queues = &self.shared.local_queues;
            let num = local_queues.len();
            let start = rand::thread_rng().gen_range(0..num);
            for i in 0..num {
                let i = (start + i) % num;
                if let Some(another) = local_queues.get(i) {
                    if !self.can_steal() {
                        //本地队列超过一半，不再steal
                        break;
                    }
                    if std::ptr::eq(&another, &self.queue) {
                        //不能偷自己
                        continue;
                    }
                    for entry in another {
                        let worker = entry.value();
                        if worker.is_empty() {
                            //其他队列为空
                            continue;
                        }
                        let into_entry = self.queue.get_or_insert_with(*entry.key(), || {
                            Worker::new(self.shared.local_capacity)
                        });
                        let into_queue = into_entry.value();
                        if worker
                            .stealer()
                            .steal(into_queue, |n| {
                                //可偷取的最大长度与本地队列可偷长度做比较
                                n.min(self.max_steal())
                                    //与其他队列当前长度的一半做比较
                                    .min(
                                        worker
                                            .capacity()
                                            .saturating_sub(worker.spare_capacity())
                                            .saturating_add(1)
                                            .saturating_div(2),
                                    )
                            })
                            .is_ok()
                        {
                            self.release_lock();
                            return self.pop_local();
                        }
                    }
                }
            }
            self.release_lock();
        }
        //都steal不到，只好从shared里pop
        self.shared.pop()
    }

    fn pop_local(&self) -> Option<T> {
        //从本地队列弹出元素
        for entry in self.queue {
            if let Some(val) = entry.value().pop() {
                // Decrement the count.
                self.len
                    .store(self.len().saturating_sub(1), Ordering::Release);
                return Some(val);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordered_work_steal_queue() {
        let queue = OrderedWorkStealQueue::new(2, 64);
        for i in 6..8 {
            queue.push_with_priority(i, i);
        }
        let local0 = queue.local_queue();
        for i in 2..6 {
            local0.push_with_priority(i, i);
        }
        let local1 = queue.local_queue();
        for i in 0..2 {
            local1.push_with_priority(i, i);
        }
        for i in 0..8 {
            assert_eq!(local1.pop(), Some(i));
        }
        assert_eq!(local0.pop(), None);
        assert_eq!(local1.pop(), None);
        assert_eq!(queue.pop(), None);
    }
}
