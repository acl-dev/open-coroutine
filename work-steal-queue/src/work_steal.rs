use crate::rand::{FastRand, RngSeedGenerator};
use crate::{Injector, Steal, Worker};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue<T> {
    shared_queue: Injector<T>,
    /// Number of pending tasks in the queue. This helps prevent unnecessary
    /// locking in the hot path.
    len: AtomicUsize,
    stealing: AtomicBool,
    local_queues: Box<[Worker<T>]>,
    index: AtomicUsize,
    seed_generator: RngSeedGenerator,
}

impl<T> Drop for WorkStealQueue<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            for local_queue in self.local_queues.iter() {
                assert!(local_queue.pop().is_none(), "local queue not empty");
            }
            assert!(self.pop().is_none(), "global queue not empty");
        }
    }
}

unsafe impl<T: Send> Send for WorkStealQueue<T> {}
unsafe impl<T: Send> Sync for WorkStealQueue<T> {}

impl<T> WorkStealQueue<T> {
    pub fn new(local_queues: usize, local_capacity: usize) -> Self {
        WorkStealQueue {
            shared_queue: Injector::new(),
            len: AtomicUsize::new(0),
            stealing: AtomicBool::new(false),
            local_queues: (0..local_queues)
                .map(|_| Worker::new_capacity_fifo(local_capacity, false))
                .collect(),
            index: AtomicUsize::new(0),
            seed_generator: RngSeedGenerator::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    pub fn push(&self, item: T) {
        self.shared_queue.push(item);
        //add count
        self.len.store(self.len() + 1, Ordering::Release);
    }

    pub fn pop(&self) -> Option<T> {
        // Fast path, if len == 0, then there are no values
        if self.is_empty() {
            return None;
        }
        loop {
            match self.shared_queue.steal() {
                Steal::Success(item) => {
                    // Decrement the count.
                    self.len.store(self.len() - 1, Ordering::Release);
                    return Some(item);
                }
                Steal::Retry => continue,
                Steal::Empty => return None,
            }
        }
    }

    fn try_lock(&self) -> bool {
        self.stealing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    fn release_lock(&self) {
        self.stealing.store(false, Ordering::Relaxed);
    }

    pub fn next_index(&self) -> usize {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        if index == usize::MAX {
            self.index.store(0, Ordering::Relaxed);
        }
        index % self.local_queues.len()
    }

    pub fn local_queue(&self) -> LocalQueue<T> {
        let local = self.local_queues.get(self.next_index()).unwrap();
        LocalQueue::new(self, local, FastRand::new(self.seed_generator.next_seed()))
    }
}

impl<T> Default for WorkStealQueue<T> {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct LocalQueue<'l, T> {
    /// Used to schedule bookkeeping tasks every so often.
    tick: AtomicU32,
    shared: &'l WorkStealQueue<T>,
    stealing: AtomicBool,
    queue: &'l Worker<T>,
    /// Fast random number generator.
    rand: FastRand,
}

impl<T> Drop for LocalQueue<'_, T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pop_front().is_none(), "local queue not empty");
        }
    }
}

unsafe impl<T: Send> Send for LocalQueue<'_, T> {}
unsafe impl<T: Send> Sync for LocalQueue<'_, T> {}

impl<'l, T> LocalQueue<'l, T> {
    pub(crate) fn new(shared: &'l WorkStealQueue<T>, queue: &'l Worker<T>, rand: FastRand) -> Self {
        LocalQueue {
            tick: AtomicU32::new(0),
            shared,
            stealing: AtomicBool::new(false),
            queue,
            rand,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.queue.cap() == self.queue.len()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    fn try_lock(&self) -> bool {
        self.stealing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    fn release_lock(&self) {
        self.stealing.store(false, Ordering::Relaxed);
    }

    /// If the queue is full, first push half to global,
    /// then push the item to global.
    ///
    /// # Examples
    ///
    /// ```
    /// use work_steal_queue::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     local.push_back(i);
    /// }
    /// assert_eq!(local.pop_front(), Some(3));
    /// assert_eq!(local.pop_front(), Some(0));
    /// assert_eq!(local.pop_front(), Some(1));
    /// assert_eq!(local.pop_front(), Some(2));
    /// assert_eq!(local.pop_front(), None);
    /// ```
    pub fn push_back(&self, item: T) -> std::io::Result<()> {
        if let Err(item) = self.queue.push(item) {
            //把本地队列的一半放到全局队列
            let count = self.len() / 2;
            let stealer = self.queue.stealer();
            for _ in 0..count {
                loop {
                    match stealer.steal() {
                        Steal::Success(v) => self.shared.push(v),
                        Steal::Retry => continue,
                        Steal::Empty => break,
                    }
                }
            }
            //直接放到全局队列
            self.shared.push(item);
        }
        Ok(())
    }

    /// Increment the tick
    fn tick(&self) -> u32 {
        let val = self.tick.fetch_add(1, Ordering::Release);
        if val == u32::MAX {
            self.tick.store(0, Ordering::Release);
            return 0;
        }
        val + 1
    }

    /// If the queue is empty, first try steal from global,
    /// then try steal from siblings.
    ///
    /// # Examples
    ///
    /// ```
    /// use work_steal_queue::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 32);
    /// queue.push(1);
    /// queue.push(2);
    /// let local = queue.local_queue();
    /// assert_eq!(local.pop_front(), Some(1));
    /// assert_eq!(local.pop_front(), Some(2));
    /// assert_eq!(local.pop_front(), None);
    /// ```
    ///
    /// # Examples
    /// ```
    /// use work_steal_queue::WorkStealQueue;
    /// let queue = WorkStealQueue::new(2, 64);
    /// let local0 = queue.local_queue();
    /// local0.push_back(2);
    /// local0.push_back(3);
    /// let local1 = queue.local_queue();
    /// local1.push_back(0);
    /// local1.push_back(1);
    /// for i in 0..4 {
    ///     assert_eq!(local1.pop_front(), Some(i));
    /// }
    /// assert_eq!(local0.pop_front(), None);
    /// assert_eq!(local1.pop_front(), None);
    /// assert_eq!(queue.pop(), None);
    /// ```
    pub fn pop_front(&self) -> Option<T> {
        //每从本地弹出61次，就从全局队列弹出
        if self.tick() % 61 == 0 {
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
            let start = self.rand.fastrand_n(num as u32) as usize;
            for i in 0..num {
                let i = (start + i) % num;
                let another: &Worker<T> = local_queues.get(i).expect("get local queue failed!");
                if let Steal::Success(popped_item) =
                    another.stealer().steal_batch_and_pop(self.queue)
                {
                    self.release_lock();
                    return Some(popped_item);
                }
            }

            //尝试从全局队列steal
            if !self.shared.is_empty() && self.shared.try_lock() {
                if let Steal::Success(popped_item) =
                    self.shared.shared_queue.steal_batch_and_pop(self.queue)
                {
                    self.shared.release_lock();
                    self.release_lock();
                    return Some(popped_item);
                }
                self.shared.release_lock();
            }
            self.release_lock();
        }
        //都steal不到，只好从shared里pop
        self.shared.pop()
    }
}
