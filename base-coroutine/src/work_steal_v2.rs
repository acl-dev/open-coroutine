use crate::random::Rng;
use concurrent_queue::{ConcurrentQueue, PopError, PushError};
use st3::fifo::Worker;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Error type returned by steal methods.
#[derive(Debug)]
pub enum StealError {
    CanNotStealSelf,
    EmptySibling,
    NoMoreSpare,
    StealSiblingFailed,
}

impl Display for StealError {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        match *self {
            StealError::CanNotStealSelf => write!(fmt, "can not steal self"),
            StealError::EmptySibling => write!(fmt, "the sibling is empty"),
            StealError::NoMoreSpare => write!(fmt, "self has no more spare"),
            StealError::StealSiblingFailed => write!(fmt, "steal from another local queue failed"),
        }
    }
}

impl Error for StealError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue<T> {
    shared_queue: Arc<ConcurrentQueue<T>>,
    shared_lock: AtomicBool,
    local_queues: Vec<Arc<LocalQueue<T>>>,
    index: AtomicUsize,
}

impl<T> WorkStealQueue<T>
where
    T: Debug,
{
    pub fn new(local_queues: usize, local_capacity: usize) -> Self {
        let global_queue = Arc::new(ConcurrentQueue::<T>::unbounded());
        let mut queue = WorkStealQueue {
            shared_queue: Arc::clone(&global_queue),
            shared_lock: AtomicBool::new(false),
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

    pub fn push(&self, item: T) -> Result<(), PushError<T>> {
        self.shared_queue.push(item)
    }

    pub fn pop(&self) -> Result<T, PopError> {
        self.shared_queue.pop()
    }

    pub(crate) fn try_lock(&self) -> bool {
        self.shared_lock
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    pub(crate) fn release_lock(&self) {
        self.shared_lock.store(false, Ordering::Relaxed);
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

impl<T> Default for WorkStealQueue<T>
where
    T: Debug,
{
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

impl<T> LocalQueue<T>
where
    T: Debug,
{
    pub fn new(shared: Arc<WorkStealQueue<T>>, max_capacity: usize) -> Self {
        LocalQueue {
            shared,
            stealing: AtomicBool::new(false),
            queue: Worker::new(max_capacity),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.capacity() - self.spare()
    }

    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    pub fn spare(&self) -> usize {
        self.queue.spare_capacity()
    }

    pub fn push_back(&self, item: T) -> std::io::Result<()> {
        if let Err(item) = self.queue.push(item) {
            //把本地队列的一半放到全局队列
            let count = self.len() / 2;
            //todo 这里实际上可以减少一次copy
            let half = Worker::new(count);
            let stealer = self.queue.stealer();
            let _ = stealer.steal(&half, |_n| count);
            while !half.is_empty() {
                let _ = self.shared.push(half.pop().unwrap());
            }
            self.shared.push(item).map_err(|e| match e {
                PushError::Full(_) => {
                    std::io::Error::new(std::io::ErrorKind::Other, "global queue is full")
                }
                PushError::Closed(_) => {
                    std::io::Error::new(std::io::ErrorKind::Other, "global queue closed")
                }
            })?
        }
        Ok(())
    }

    pub(crate) fn try_lock(&self) -> bool {
        self.stealing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    pub(crate) fn release_lock(&self) {
        self.stealing.store(false, Ordering::Relaxed);
    }

    pub(crate) fn steal_siblings(
        &self,
        another: &Arc<LocalQueue<T>>,
        count: usize,
    ) -> Result<(), StealError> {
        if std::ptr::eq(&another.queue, &self.queue) {
            return Err(StealError::CanNotStealSelf);
        }
        if another.is_empty() {
            return Err(StealError::EmptySibling);
        }
        let count = (another.len() / 2)
            .min(self.queue.spare_capacity())
            .min(count);
        if count == 0 {
            return Err(StealError::NoMoreSpare);
        }
        another
            .queue
            .stealer()
            .steal(&self.queue, |_n| count)
            .map_err(|_| StealError::StealSiblingFailed)
            .map(|_| ())
    }

    pub(crate) fn steal_global(&self, count: usize) {
        let count = count.min(self.queue.spare_capacity());
        for _ in 0..count {
            match self.shared.pop() {
                Ok(item) => self.queue.push(item).expect("steal to local queue failed!"),
                Err(_) => break,
            }
        }
        self.shared.release_lock();
    }

    pub fn pop_front(&self) -> Option<T> {
        //优先从本地队列弹出元素
        if let Some(val) = self.queue.pop() {
            return Some(val);
        }
        if self.try_lock() {
            //尝试从全局队列steal
            if self.shared.try_lock() {
                if let Ok(popped_item) = self.shared.pop() {
                    self.steal_global(self.queue.capacity() / 2);
                    self.release_lock();
                    return Some(popped_item);
                }
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
                let random = Rng {
                    state: timer_utils::now(),
                }
                .gen_usize_to(len);
                indexes.swap(i, random);
            }
            for i in indexes {
                let another = local_queues.get(i).expect("get local queue failed!");
                if self.steal_siblings(another, usize::MAX).is_ok() {
                    self.release_lock();
                    return self.queue.pop();
                }
            }
            self.release_lock();
        }
        match self.shared.pop() {
            Ok(item) => Some(item),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkStealQueue;

    #[test]
    fn push_many() {
        let queue = WorkStealQueue::new(1, 2);
        let mut local = queue.local_queue();
        for i in 0..4 {
            local.push_back(i);
        }
        assert_eq!(local.pop_front(), Some(1));
        assert_eq!(local.pop_front(), Some(3));
        assert_eq!(local.pop_front(), Some(0));
        assert_eq!(local.pop_front(), Some(2));
        assert_eq!(local.pop_front(), None);
    }

    #[test]
    fn steal_global() {
        let queue = WorkStealQueue::new(1, 32);
        for i in 0..16 {
            queue.push(i);
        }
        let local = queue.local_queue();
        for i in 0..16 {
            assert_eq!(local.pop_front().unwrap(), i);
        }
        assert!(local.pop_front().is_none());
    }

    #[test]
    fn steal_siblings() {
        let queue = WorkStealQueue::new(2, 64);
        queue.push(2);
        queue.push(3);

        let local0 = queue.local_queue();
        local0.push_back(4);
        local0.push_back(5);
        local0.push_back(6);
        local0.push_back(7);

        let local1 = queue.local_queue();
        local1.push_back(0);
        local1.push_back(1);
        for i in 0..7 {
            assert_eq!(local1.pop_front(), Some(i));
        }
    }
}
