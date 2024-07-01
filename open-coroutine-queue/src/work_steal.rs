use crate::rand::{FastRand, RngSeedGenerator};
use crossbeam_deque::{Injector, Steal};
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
    seed_generator: RngSeedGenerator,
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
    /// Get a global `WorkStealQueue` instance.
    #[allow(unsafe_code, trivial_casts)]
    pub fn get_instance<'s>() -> &'s WorkStealQueue<T> {
        static INSTANCE: AtomicUsize = AtomicUsize::new(0);
        let mut ret = INSTANCE.load(Ordering::Relaxed);
        if ret == 0 {
            let ptr: &'s mut WorkStealQueue<T> = Box::leak(Box::default());
            ret = std::ptr::from_mut::<WorkStealQueue<T>>(ptr) as usize;
            INSTANCE.store(ret, Ordering::Relaxed);
        }
        unsafe { &*(ret as *mut WorkStealQueue<T>) }
    }

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
            seed_generator: RngSeedGenerator::default(),
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
        self.len.store(self.len() + 1, Ordering::Release);
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
                    self.len.store(self.len() - 1, Ordering::Release);
                    return Some(item);
                }
                Steal::Retry => continue,
                Steal::Empty => return None,
            }
        }
    }

    /// Get a local queue, this method should be called up to `local_queue_size` times.
    ///
    /// # Panics
    /// should never happens
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
        LocalQueue::new(self, local, FastRand::new(self.seed_generator.next_seed()))
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
    /// Fast random number generator.
    rand: FastRand,
}

impl<T: Debug> Default for LocalQueue<'_, T> {
    fn default() -> Self {
        WorkStealQueue::get_instance().local_queue()
    }
}

impl<T: Debug> Drop for LocalQueue<'_, T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.queue.pop().is_none(), "local queue not empty");
        }
    }
}

impl<'l, T: Debug> LocalQueue<'l, T> {
    fn new(shared: &'l WorkStealQueue<T>, queue: &'l Worker<T>, rand: FastRand) -> Self {
        LocalQueue {
            tick: AtomicU32::new(0),
            shared,
            stealing: AtomicBool::new(false),
            queue,
            rand,
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
    /// use open_coroutine_queue::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// assert!(local.is_empty());
    /// for i in 0..2 {
    ///     local.push_back(i);
    /// }
    /// assert!(local.is_full());
    /// assert_eq!(local.pop_front(), Some(0));
    /// assert_eq!(local.len(), 1);
    /// assert_eq!(local.pop_front(), Some(1));
    /// assert_eq!(local.pop_front(), None);
    /// assert!(local.is_empty());
    /// ```
    pub fn is_full(&self) -> bool {
        self.queue.spare_capacity() == 0
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        self.queue.capacity() - self.queue.spare_capacity()
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
    /// use open_coroutine_queue::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 2);
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     local.push_back(i);
    /// }
    /// assert_eq!(local.pop_front(), Some(1));
    /// assert_eq!(local.pop_front(), Some(3));
    /// assert_eq!(local.pop_front(), Some(0));
    /// assert_eq!(local.pop_front(), Some(2));
    /// assert_eq!(local.pop_front(), None);
    /// ```
    pub fn push_back(&self, item: T) {
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
        val + 1
    }

    /// If the queue is empty, first try steal from global,
    /// then try steal from siblings.
    ///
    /// # Examples
    ///
    /// ```
    /// use open_coroutine_queue::WorkStealQueue;
    ///
    /// let queue = WorkStealQueue::new(1, 32);
    /// for i in 0..4 {
    ///     queue.push(i);
    /// }
    /// let local = queue.local_queue();
    /// for i in 0..4 {
    ///     assert_eq!(local.pop_front(), Some(i));
    /// }
    /// assert_eq!(local.pop_front(), None);
    /// assert_eq!(queue.pop(), None);
    /// ```
    ///
    /// # Examples
    /// ```
    /// use open_coroutine_queue::WorkStealQueue;
    /// let queue = WorkStealQueue::new(2, 64);
    /// let local0 = queue.local_queue();
    /// local0.push_back(2);
    /// local0.push_back(3);
    /// local0.push_back(4);
    /// local0.push_back(5);
    /// let local1 = queue.local_queue();
    /// local1.push_back(0);
    /// local1.push_back(1);
    /// for i in 0..6 {
    ///     assert_eq!(local1.pop_front(), Some(i));
    /// }
    /// assert_eq!(local0.pop_front(), None);
    /// assert_eq!(local1.pop_front(), None);
    /// assert_eq!(queue.pop(), None);
    /// ```
    #[allow(clippy::cast_possible_truncation)]
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
                if let Some(another) = local_queues.get(i) {
                    if std::ptr::eq(&another, &self.queue) {
                        //不能偷自己
                        continue;
                    }
                    if another.is_empty() {
                        //其他队列为空
                        continue;
                    }
                    if self.queue.spare_capacity() == 0 {
                        //本地队列已满
                        continue;
                    }
                    if another
                        .stealer()
                        .steal(self.queue, |n| {
                            //可偷取的最大长度与本地队列空闲长度做比较
                            n.min(self.queue.spare_capacity())
                                //与其他队列当前长度的一半做比较
                                .min(((another.capacity() - another.spare_capacity()) + 1) / 2)
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
