use crate::random::Rng;
use concurrent_queue::ConcurrentQueue;
use once_cell::sync::{Lazy, OnceCell};
use st3::fifo::Worker;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

static mut INSTANCE: Lazy<Queue> = Lazy::new(Queue::default);

pub fn get_queue() -> &'static mut WorkStealQueue {
    unsafe { INSTANCE.local_queue() }
}

static mut GLOBAL_QUEUE: Lazy<ConcurrentQueue<*mut c_void>> = Lazy::new(ConcurrentQueue::unbounded);

static mut LOCAL_QUEUES: OnceCell<Box<[WorkStealQueue]>> = OnceCell::new();

#[repr(C)]
#[derive(Debug)]
pub struct Queue {
    index: AtomicUsize,
}

impl Queue {
    pub fn new(local_queues: usize, local_capacity: usize) -> Self {
        unsafe {
            LOCAL_QUEUES.get_or_init(|| {
                (0..local_queues)
                    .map(|_| WorkStealQueue::new(local_capacity))
                    .collect()
            });
        }
        Queue {
            index: AtomicUsize::new(0),
        }
    }

    /// Push an item to the global queue. When one of the local queues empties, they can pick this
    /// item up.
    pub fn push<T>(&self, item: T) {
        let ptr = Box::leak(Box::new(item));
        self.push_raw(ptr as *mut _ as *mut c_void)
    }

    pub fn push_raw(&self, ptr: *mut c_void) {
        unsafe { GLOBAL_QUEUE.push(ptr).unwrap() }
    }

    pub fn local_queue(&mut self) -> &mut WorkStealQueue {
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

impl Default for Queue {
    fn default() -> Self {
        Self::new(num_cpus::get(), 256)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue {
    queue: Worker<*mut c_void>,
}

impl WorkStealQueue {
    fn new(max_capacity: usize) -> Self {
        WorkStealQueue {
            queue: Worker::new(max_capacity),
        }
    }

    pub fn push_back<T>(&mut self, element: T) {
        let ptr = Box::leak(Box::new(element));
        self.push_back_raw(ptr as *mut _ as *mut c_void);
    }

    pub fn push_back_raw(&mut self, ptr: *mut c_void) {
        if let Err(item) = self.queue.push(ptr) {
            unsafe {
                //把本地队列的一半放到全局队列
                let count = self.len() / 2;
                //todo 这里实际上可以减少一次copy
                let half = Worker::new(count);
                let stealer = self.queue.stealer();
                stealer
                    .steal(&half, |_n| count)
                    .expect("steal half to global failed !");
                while !half.is_empty() {
                    GLOBAL_QUEUE
                        .push(half.pop().unwrap())
                        .expect("push half to global queue failed!");
                }
                GLOBAL_QUEUE
                    .push(item)
                    .expect("push to global queue failed!")
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.capacity() - self.queue.spare_capacity()
    }

    /// 如果是闭包，还是要获取裸指针再手动转换，不然类型有问题
    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        //优先从本地队列弹出元素
        if let Some(val) = self.queue.pop() {
            return Some(val);
        }
        unsafe {
            //从全局队列steal
            if let Ok(popped_item) = GLOBAL_QUEUE.pop() {
                let count = (self.queue.capacity() / 2).min(self.queue.spare_capacity());
                for _ in 0..count {
                    match GLOBAL_QUEUE.pop() {
                        Ok(item) => self.queue.push(item).expect("steal to local queue failed!"),
                        Err(_) => break,
                    }
                }
                return Some(popped_item);
            }
            //从其他本地队列steal
            let local_queues = LOCAL_QUEUES.get_mut().unwrap();
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
                let another: &mut WorkStealQueue =
                    local_queues.get_mut(i).expect("get local queue failed!");
                if std::ptr::eq(&another.queue, &self.queue) {
                    continue;
                }
                if another.is_empty() {
                    continue;
                }
                let stealer = another.queue.stealer();
                let count = (another.len() / 2).min(self.queue.spare_capacity());
                if count == 0 {
                    continue;
                }
                stealer
                    .steal(&self.queue, |_n| count)
                    .expect("steal half from another local queue failed !");
                return self.queue.pop();
            }
            match GLOBAL_QUEUE.pop() {
                Ok(item) => Some(item),
                Err(_) => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use std::os::raw::c_void;
    //
    // #[test]
    // fn push_less() {
    //     let mut queue = Queue::new(1, 2);
    //     let local = queue.local_queue();
    //     for i in 0..2 {
    //         local.push_back_raw(i as *mut c_void);
    //     }
    //     assert_eq!(local.pop_front_raw().unwrap(), 0 as *mut c_void);
    //     assert_eq!(local.pop_front_raw().unwrap(), 1 as *mut c_void);
    //     assert_eq!(local.pop_front_raw(), None);
    // }
    //
    // #[test]
    // fn push_many() {
    //     let mut queue = Queue::new(1, 2);
    //     let local = queue.local_queue();
    //     for i in 0..4 {
    //         local.push_back_raw(i as *mut c_void);
    //     }
    //     assert_eq!(local.pop_front_raw().unwrap(), 1 as *mut c_void);
    //     assert_eq!(local.pop_front_raw().unwrap(), 3 as *mut c_void);
    //     assert_eq!(local.pop_front_raw().unwrap(), 0 as *mut c_void);
    //     assert_eq!(local.pop_front_raw().unwrap(), 2 as *mut c_void);
    //     assert_eq!(local.pop_front_raw(), None);
    // }
    //
    // #[test]
    // fn wrapping() {
    //     let mut queue = Queue::new(1, 2);
    //     let local = queue.local_queue();
    //
    //     local.push_back_raw(0 as *mut c_void);
    //
    //     for i in 0..10 {
    //         local.push_back_raw((i + 1) as *mut c_void);
    //
    //         assert_eq!(local.pop_front_raw(), Some(i as *mut c_void));
    //     }
    //
    //     assert_eq!(local.pop_front_raw(), Some(10 as *mut c_void));
    //     assert_eq!(local.pop_front_raw(), None);
    //     assert_eq!(local.pop_front_raw(), None);
    // }
    //
    // #[test]
    // fn steal_global() {
    //     for &size in &[2, 4, 8, 16, 32, 64] {
    //         let mut queue = Queue::new(4, size);
    //
    //         for i in 0..16 {
    //             queue.push_raw(i as *mut c_void);
    //         }
    //
    //         let local = queue.local_queue();
    //
    //         for i in 0..16 {
    //             assert_eq!(local.pop_front_raw().unwrap(), i as *mut c_void);
    //         }
    //
    //         assert_eq!(local.pop_front_raw(), None);
    //     }
    // }
    //
    // #[test]
    // fn steal_siblings() {
    //     let queue = Queue::new(2, 64);
    //
    //     let locals = unsafe { LOCAL_QUEUES.get_mut().unwrap() };
    //
    //     locals[0].push_back_raw(4 as *mut c_void);
    //     locals[0].push_back_raw(5 as *mut c_void);
    //     locals[0].push_back_raw(6 as *mut c_void);
    //     locals[0].push_back_raw(7 as *mut c_void);
    //
    //     locals[1].push_back_raw(0 as *mut c_void);
    //     locals[1].push_back_raw(1 as *mut c_void);
    //
    //     queue.push_raw(2 as *mut c_void);
    //     queue.push_raw(3 as *mut c_void);
    //
    //     for i in 0..6 {
    //         assert_eq!(locals[1].pop_front_raw().unwrap(), i as *mut c_void);
    //     }
    // }
    //
    // #[test]
    // fn test_work_steal_queue() {
    //     let queue = get_queue();
    //     assert!(queue.is_empty());
    //     queue.push_back_raw(1usize as *mut c_void);
    //     assert_eq!(1, queue.len());
    //     assert_eq!(1usize as *mut c_void, queue.pop_front_raw().unwrap());
    // }
}
