use crate::{Injector, Steal, Worker};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue<T> {
    shared_queue: Arc<Injector<T>>,
    stealing: AtomicBool,
    local_queues: Vec<Arc<LocalQueue<T>>>,
    index: AtomicUsize,
}

unsafe impl<T: Send> Send for WorkStealQueue<T> {}
unsafe impl<T: Send> Sync for WorkStealQueue<T> {}

impl<T> WorkStealQueue<T> {
    pub fn new(local_queues: usize, local_capacity: usize) -> Self {
        let global_queue = Arc::new(Injector::new());
        let mut queue = WorkStealQueue {
            shared_queue: Arc::clone(&global_queue),
            stealing: AtomicBool::new(false),
            local_queues: Vec::with_capacity(local_queues),
            index: AtomicUsize::new(0),
        };
        for _ in 0..local_queues {
            queue.local_queues.push(Arc::new(LocalQueue::new(
                unsafe { Arc::from_raw(&queue) },
                local_capacity,
            )));
        }
        queue
    }

    pub fn push(&self, item: T) {
        self.shared_queue.push(item)
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            match self.shared_queue.steal() {
                Steal::Success(item) => return Some(item),
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

    pub fn local_queue(&self) -> Arc<LocalQueue<T>> {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        if index == usize::MAX {
            self.index.store(0, Ordering::Relaxed);
        }
        let local = self.local_queues.get(index % num_cpus::get()).unwrap();
        Arc::clone(local)
    }
}

impl<T> Default for WorkStealQueue<T> {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct LocalQueue<T> {
    shared: Arc<WorkStealQueue<T>>,
    stealing: AtomicBool,
    queue: Worker<T>,
}

unsafe impl<T: Send> Send for LocalQueue<T> {}
unsafe impl<T: Send> Sync for LocalQueue<T> {}

impl<T> LocalQueue<T> {
    pub fn new(shared: Arc<WorkStealQueue<T>>, max_capacity: usize) -> Self {
        LocalQueue {
            shared,
            stealing: AtomicBool::new(false),
            queue: Worker::new_capacity_fifo(max_capacity, false),
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
    pub fn push_back(&self, item: T) {
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
    /// assert!(local.pop_front().is_none());
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
    /// ```
    pub fn pop_front(&self) -> Option<T> {
        //优先从本地队列弹出元素
        if let Some(val) = self.queue.pop() {
            return Some(val);
        }
        if self.try_lock() {
            //尝试从全局队列steal
            if self.shared.try_lock() {
                if let Steal::Success(popped_item) =
                    self.shared.shared_queue.steal_batch_and_pop(&self.queue)
                {
                    self.shared.release_lock();
                    self.release_lock();
                    return Some(popped_item);
                }
                self.shared.release_lock();
            }

            //尝试从其他本地队列steal
            let local_queues = &self.shared.local_queues;
            //这里生成一个打乱顺序的数组，遍历获取index
            let mut indexes = Vec::new();
            let len = local_queues.len();
            for i in 0..len {
                indexes.push(i);
            }
            for i in 0..(len / 2) {
                let random = crate::random::Rng {
                    state: timer_utils::now(),
                }
                .gen_usize_to(len);
                indexes.swap(i, random);
            }
            for i in indexes {
                let another = local_queues.get(i).expect("get local queue failed!");
                if self.is_full() {
                    // self has no more space
                    break;
                }
                if let Steal::Success(popped_item) =
                    another.queue.stealer().steal_batch_and_pop(&self.queue)
                {
                    self.release_lock();
                    return Some(popped_item);
                }
            }
            self.release_lock();
        }
        //都steal不到，只好从shared里pop
        self.shared.pop()
    }
}
