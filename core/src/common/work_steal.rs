use crossbeam_deque::{Injector, Steal};
use rand::Rng;
use st3::fifo::Worker;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Work stealing global queue, shared by multiple threads.
#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue<T: Debug> {
    shared_queue: Injector<T>,
    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,
    local_queues: VecDeque<Worker<T>>,
    index: AtomicUsize,
}

impl<T: Debug> Drop for WorkStealQueue<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            for local_queue in &self.local_queues {
                assert!(local_queue.pop().is_none(), "local queue not empty");
            }
            assert!(self.pop().is_none(), "global queue not empty");
        }
    }
}

impl<T: Debug> WorkStealQueue<T> {
    /// Create a new `WorkStealQueue` instance.
    #[must_use]
    pub fn new(local_queues_size: usize, local_capacity: usize) -> Self {
        WorkStealQueue {
            shared_queue: Injector::new(),
            len: AtomicUsize::new(0),
            local_queues: (0..local_queues_size)
                .map(|_| Worker::new(local_capacity))
                .collect(),
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
    pub fn push(&self, item: T) {
        self.shared_queue.push(item);
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
        loop {
            match self.shared_queue.steal() {
                Steal::Success(item) => {
                    // Decrement the count.
                    self.len
                        .store(self.len().saturating_sub(1), Ordering::Release);
                    return Some(item);
                }
                Steal::Retry => {}
                Steal::Empty => return None,
            }
        }
    }

    /// Get a local queue, this method should be called up to `local_queue_size` times.
    ///
    /// # Panics
    /// should never happen
    pub fn local_queue(&self) -> LocalQueue<'_, T> {
        let mut index = self.index.fetch_add(1, Ordering::Relaxed);
        if index == usize::MAX {
            self.index.store(0, Ordering::Relaxed);
        }
        index %= self.local_queues.len();
        let local = self
            .local_queues
            .get(index)
            .unwrap_or_else(|| panic!("local queue {index} init failed!"));
        LocalQueue::new(self, local)
    }
}

impl<T: Debug> Default for WorkStealQueue<T> {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

/// The work stealing local queue, exclusive to thread.
#[repr(C)]
#[derive(Debug)]
pub struct LocalQueue<'l, T: Debug> {
    /// Used to schedule bookkeeping tasks every so often.
    tick: AtomicU32,
    shared: &'l WorkStealQueue<T>,
    stealing: AtomicBool,
    queue: &'l Worker<T>,
}

impl<T: Debug> Drop for LocalQueue<'_, T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.queue.pop().is_none(), "local queue not empty");
        }
    }
}

impl<'l, T: Debug> LocalQueue<'l, T> {
    fn new(shared: &'l WorkStealQueue<T>, queue: &'l Worker<T>) -> Self {
        LocalQueue {
            tick: AtomicU32::new(0),
            shared,
            stealing: AtomicBool::new(false),
            queue,
        }
    }

    /// Returns `true` if the local queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns `true` if the local queue is full.
    ///
    /// # Examples
    ///
    /// ```
    /// use open_coroutine_core::common::work_steal::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// assert!(local.is_empty());
    /// for i in 0..2 {
    ///     local.push(i);
    /// }
    /// assert!(local.is_full());
    /// assert_eq!(local.pop(), Some(0));
    /// assert_eq!(local.len(), 1);
    /// assert_eq!(local.pop(), Some(1));
    /// assert_eq!(local.pop(), None);
    /// assert!(local.is_empty());
    /// ```
    pub fn is_full(&self) -> bool {
        self.queue.spare_capacity() == 0
    }

    fn max_steal(&self) -> usize {
        self.queue
            .capacity()
            .saturating_add(1)
            .saturating_div(2)
            .saturating_sub(self.len())
    }

    fn can_steal(&self) -> bool {
        self.queue.spare_capacity() >= self.queue.capacity().saturating_add(1).saturating_div(2)
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        self.queue
            .capacity()
            .saturating_sub(self.queue.spare_capacity())
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
    /// use open_coroutine_core::common::work_steal::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     local.push(i);
    /// }
    /// assert_eq!(local.pop(), Some(1));
    /// assert_eq!(local.pop(), Some(3));
    /// assert_eq!(local.pop(), Some(0));
    /// assert_eq!(local.pop(), Some(2));
    /// assert_eq!(local.pop(), None);
    /// ```
    pub fn push(&self, item: T) {
        if let Err(item) = self.queue.push(item) {
            //把本地队列的一半放到全局队列
            let count = self.len() / 2;
            for _ in 0..count {
                if let Some(item) = self.queue.pop() {
                    self.shared.push(item);
                }
            }
            //直接放到全局队列
            self.shared.push(item);
        }
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
    /// use open_coroutine_core::common::work_steal::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 32);
    /// for i in 0..4 {
    ///     queue.push(i);
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
    /// use open_coroutine_core::common::work_steal::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(2, 64);
    /// let local0 = queue.local_queue();
    /// local0.push(2);
    /// local0.push(3);
    /// local0.push(4);
    /// local0.push(5);
    /// assert_eq!(local0.len(), 4);
    /// let local1 = queue.local_queue();
    /// local1.push(0);
    /// local1.push(1);
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
        if self.tick().is_multiple_of(61) {
            if let Some(val) = self.shared.pop() {
                return Some(val);
            }
        }
        //从本地队列弹出元素
        if let Some(val) = self.queue.pop() {
            return Some(val);
        }
        if self.try_lock() {
            //尝试从其他本地队列steal
            let local_queues = &self.shared.local_queues;
            let num = local_queues.len();
            let start = rand::rng().random_range(0..num);
            for i in 0..num {
                let i = (start + i) % num;
                if let Some(another) = local_queues.get(i) {
                    if !self.can_steal() {
                        //本地队列超过一半，不再steal
                        break;
                    }
                    if std::ptr::eq(&raw const another, &raw const self.queue) {
                        //不能偷自己
                        continue;
                    }
                    if another.is_empty() {
                        //其他队列为空
                        continue;
                    }
                    if another
                        .stealer()
                        .steal(self.queue, |n| {
                            //可偷取的最大长度与本地队列可偷长度做比较
                            n.min(self.max_steal())
                                //与其他队列当前长度的一半做比较
                                .min(
                                    another
                                        .capacity()
                                        .saturating_sub(another.spare_capacity())
                                        .saturating_add(1)
                                        .saturating_div(2),
                                )
                        })
                        .is_ok()
                    {
                        self.release_lock();
                        return self.queue.pop();
                    }
                }
            }
            self.release_lock();
        }
        //都steal不到，只好从shared里pop
        self.shared.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_steal_queue() {
        let queue = WorkStealQueue::new(2, 64);
        queue.push(6);
        queue.push(7);

        let local0 = queue.local_queue();
        local0.push(2);
        local0.push(3);
        local0.push(4);
        local0.push(5);

        let local1 = queue.local_queue();
        local1.push(0);
        local1.push(1);
        for i in 0..8 {
            assert_eq!(local1.pop(), Some(i));
        }
        assert_eq!(local0.pop(), None);
        assert_eq!(local1.pop(), None);
        assert_eq!(queue.pop(), None);
    }
}
