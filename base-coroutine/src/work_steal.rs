use concurrent_queue::{ConcurrentQueue, PushError};
use once_cell::sync::{Lazy, OnceCell};
use st3::fifo::Worker;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use work_steal_queue::rand::{FastRand, RngSeedGenerator};

static mut INSTANCE: Lazy<WorkStealQueue> = Lazy::new(WorkStealQueue::default);

pub fn get_queue() -> &'static mut LocalQueue {
    unsafe { INSTANCE.local_queue() }
}

static mut GLOBAL_LOCK: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

pub(crate) static mut GLOBAL_QUEUE: Lazy<ConcurrentQueue<*mut c_void>> =
    Lazy::new(ConcurrentQueue::unbounded);

pub(crate) static mut LOCAL_QUEUES: OnceCell<Box<[LocalQueue]>> = OnceCell::new();

static RNG_SEED_GENERATOR: Lazy<RngSeedGenerator> = Lazy::new(RngSeedGenerator::default);

#[repr(C)]
#[derive(Debug)]
struct WorkStealQueue {
    index: AtomicUsize,
}

impl WorkStealQueue {
    fn new(local_queues: usize, local_capacity: usize) -> Self {
        unsafe {
            LOCAL_QUEUES.get_or_init(|| {
                (0..local_queues)
                    .map(|_| LocalQueue::new(local_capacity))
                    .collect()
            });
        }
        WorkStealQueue {
            index: AtomicUsize::new(0),
        }
    }

    fn local_queue(&mut self) -> &mut LocalQueue {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        if index == usize::MAX {
            self.index.store(0, Ordering::Relaxed);
        }
        unsafe {
            LOCAL_QUEUES
                .get_mut()
                .unwrap()
                .get_mut(index % num_cpus::get())
                .unwrap()
        }
    }
}

impl Default for WorkStealQueue {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

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
pub struct LocalQueue {
    stealing: AtomicBool,
    queue: Worker<*mut c_void>,
    max_capacity: usize,
    /// Used to schedule bookkeeping tasks every so often.
    tick: AtomicU32,
    /// Fast random number generator.
    rand: FastRand,
}

impl LocalQueue {
    fn new(max_capacity: usize) -> Self {
        LocalQueue {
            stealing: AtomicBool::new(false),
            queue: Worker::new(max_capacity),
            max_capacity,
            tick: AtomicU32::new(0),
            rand: FastRand::new(RNG_SEED_GENERATOR.next_seed()),
        }
    }

    pub fn push_back<T>(&mut self, element: T) -> std::io::Result<()> {
        let ptr = Box::leak(Box::new(element));
        self.push_back_raw(ptr as *mut _ as *mut c_void)
    }

    pub fn push_back_raw(&mut self, ptr: *mut c_void) -> std::io::Result<()> {
        if self.len() < self.max_capacity {
            if let Err(v) = self.queue.push(ptr) {
                self.push_overflow(v)?;
            }
        } else {
            self.push_overflow(ptr)?;
        }
        Ok(())
    }

    fn push_overflow(&mut self, item: *mut c_void) -> std::io::Result<()> {
        //把本地队列的一半放到全局队列
        let drain = self.queue.drain(|n| n / 2).unwrap();
        for v in drain {
            self.push_global_raw(v)?;
        }
        self.push_global_raw(item)
    }

    pub fn push_global<T>(&mut self, element: T) -> std::io::Result<()> {
        let ptr = Box::leak(Box::new(element));
        self.push_global_raw(ptr as *mut _ as *mut c_void)
    }

    /// Push an item to the global queue. When one of the local queues empties,
    /// they can pick this item up.
    pub fn push_global_raw(&self, ptr: *mut c_void) -> std::io::Result<()> {
        unsafe {
            GLOBAL_QUEUE.push(ptr).map_err(|e| match e {
                PushError::Full(_) => std::io::Error::new(ErrorKind::Other, "global queue is full"),
                PushError::Closed(_) => {
                    std::io::Error::new(ErrorKind::Other, "global queue closed")
                }
            })
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

    pub(crate) fn try_lock(&mut self) -> bool {
        self.stealing
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    pub(crate) fn release_lock(&mut self) {
        self.stealing.store(false, Ordering::Relaxed);
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

    /// 如果是闭包，还是要获取裸指针再手动转换，不然类型有问题
    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        //每从本地弹出61次，就从全局队列弹出
        if self.tick() % 61 == 0 {
            if let Ok(val) = unsafe { GLOBAL_QUEUE.pop() } {
                return Some(val);
            }
        }

        //从本地队列弹出元素
        if let Some(val) = self.queue.pop() {
            return Some(val);
        }
        if self.try_lock() {
            //尝试从其他本地队列steal
            let local_queues = unsafe { LOCAL_QUEUES.get_mut().unwrap() };
            let num = local_queues.len();
            let start = self.rand.fastrand_n(num as u32) as usize;
            for i in 0..num {
                let i = (start + i) % num;
                let another: &mut LocalQueue =
                    local_queues.get_mut(i).expect("get local queue failed!");
                if self.steal_siblings(another, usize::MAX).is_ok() {
                    self.release_lock();
                    return self.queue.pop();
                }
            }

            //尝试从全局队列steal
            if LocalQueue::try_global_lock() {
                if let Ok(popped_item) = unsafe { GLOBAL_QUEUE.pop() } {
                    self.steal_global(self.queue.capacity() / 2);
                    self.release_lock();
                    return Some(popped_item);
                }
            }
            self.release_lock();
        }
        match unsafe { GLOBAL_QUEUE.pop() } {
            Ok(item) => Some(item),
            Err(_) => None,
        }
    }

    pub(crate) fn steal_siblings(
        &mut self,
        another: &mut LocalQueue,
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

    pub(crate) fn try_global_lock() -> bool {
        unsafe {
            GLOBAL_LOCK
                .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        }
    }

    pub(crate) fn steal_global(&mut self, count: usize) {
        unsafe {
            let count = count.min(self.queue.spare_capacity());
            for _ in 0..count {
                match GLOBAL_QUEUE.pop() {
                    Ok(item) => self.queue.push(item).expect("steal to local queue failed!"),
                    Err(_) => break,
                }
            }
            GLOBAL_LOCK.store(false, Ordering::Relaxed);
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::os::raw::c_void;
//
//     #[test]
//     fn steal_global() {
//         for i in 0..16 {
//             unsafe {
//                 INSTANCE.push_raw(i as *mut c_void);
//             }
//         }
//         let local = get_queue();
//         for i in 0..16 {
//             assert_eq!(local.pop_front_raw().unwrap(), i as *mut c_void);
//         }
//         assert_eq!(local.pop_front_raw(), None);
//     }
//
//     #[test]
//     fn steal_siblings() {
//         unsafe {
//             INSTANCE.push_raw(2 as *mut c_void);
//             INSTANCE.push_raw(3 as *mut c_void);
//         }
//
//         let local0 = get_queue();
//         local0.push_back_raw(4 as *mut c_void);
//         local0.push_back_raw(5 as *mut c_void);
//         local0.push_back_raw(6 as *mut c_void);
//         local0.push_back_raw(7 as *mut c_void);
//
//         let local1 = get_queue();
//         local1.push_back_raw(0 as *mut c_void);
//         local1.push_back_raw(1 as *mut c_void);
//         for i in 0..7 {
//             assert_eq!(local1.pop_front_raw().unwrap(), i as *mut c_void);
//         }
//     }
// }
