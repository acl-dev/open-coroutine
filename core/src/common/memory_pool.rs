use crate::common::now;
use crossbeam_utils::atomic::AtomicCell;
use std::collections::BinaryHeap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, AtomicUsize};

/// A trait for reusable memory.
///
/// # Safety
/// The memory requires exclusive use when writing.
pub unsafe trait ReusableMemory: Ord + Clone {
    /// Create a new instance of `ReusableMemory`.
    fn new(size: usize) -> std::io::Result<Self>
    where
        Self: Sized;

    /// Get the reference count of the memory.
    fn reference_count(&self) -> usize;

    /// Get the top of the memory.
    fn top(&self) -> NonZeroUsize;

    /// Get the bottom of the memory.
    fn bottom(&self) -> NonZeroUsize;

    /// Get the size of the memory.
    fn size(&self) -> usize {
        self.top()
            .get()
            .checked_sub(self.bottom().get())
            .expect("the `bottom` is bigger than `top`")
    }

    /// Get the creation time of the memory.
    fn create_time(&self) -> u64;

    /// Callback when the memory is reused.
    fn on_reuse(&mut self) -> std::io::Result<()>;
}

/// A memory pool for reusing.
#[repr(C)]
#[derive(educe::Educe)]
#[educe(Debug)]
pub struct MemoryPool<M: ReusableMemory> {
    #[educe(Debug(ignore))]
    pool: AtomicCell<BinaryHeap<M>>,
    len: AtomicUsize,
    //最小内存数，即核心内存数
    min_count: AtomicUsize,
    //非核心内存的最大存活时间，单位ns
    keep_alive_time: AtomicU64,
}

unsafe impl<M: ReusableMemory> Send for MemoryPool<M> {}

unsafe impl<M: ReusableMemory> Sync for MemoryPool<M> {}

impl<M: ReusableMemory> Default for MemoryPool<M> {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

#[allow(missing_docs)]
impl<M: ReusableMemory> MemoryPool<M> {
    /// Create a new instance of `MemoryPool`.
    #[must_use]
    pub fn new(min_count: usize, keep_alive_time: u64) -> Self {
        Self {
            pool: AtomicCell::new(BinaryHeap::default()),
            len: AtomicUsize::default(),
            min_count: AtomicUsize::new(min_count),
            keep_alive_time: AtomicU64::new(keep_alive_time),
        }
    }

    pub fn allocate(&self, memory_size: usize) -> std::io::Result<M> {
        let heap = unsafe {
            self.pool
                .as_ptr()
                .as_mut()
                .expect("MemoryPool is not unique")
        };
        // find min memory
        let mut not_use = Vec::new();
        while let Some(memory) = heap.peek() {
            if memory.reference_count() > 1 {
                // can't use the memory
                break;
            }
            if let Some(mut memory) = heap.pop() {
                self.sub_len();
                if memory_size <= memory.size() {
                    for s in not_use {
                        heap.push(s);
                        self.add_len();
                    }
                    heap.push(memory.clone());
                    self.add_len();
                    return memory.on_reuse().map(|()| memory);
                }
                if self.min_count() < self.len()
                    && now() <= memory.create_time().saturating_add(self.keep_alive_time())
                {
                    // clean the expired memory
                    continue;
                }
                not_use.push(memory);
            }
        }
        let memory = M::new(memory_size)?;
        heap.push(memory.clone());
        self.add_len();
        Ok(memory)
    }

    pub fn set_keep_alive_time(&self, keep_alive_time: u64) -> &Self {
        self.keep_alive_time
            .store(keep_alive_time, std::sync::atomic::Ordering::Release);
        self
    }

    pub fn keep_alive_time(&self) -> u64 {
        self.keep_alive_time
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn set_min_count(&self, min_count: usize) -> &Self {
        self.min_count
            .store(min_count, std::sync::atomic::Ordering::Release);
        self
    }

    pub fn min_count(&self) -> usize {
        self.min_count.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn is_empty(&self) -> bool {
        0 == self.len()
    }

    pub fn len(&self) -> usize {
        self.len.load(std::sync::atomic::Ordering::Acquire)
    }

    fn add_len(&self) {
        self.len.store(
            self.len().saturating_add(1),
            std::sync::atomic::Ordering::Release,
        );
    }

    fn sub_len(&self) {
        self.len.store(
            self.len().saturating_sub(1),
            std::sync::atomic::Ordering::Release,
        );
    }

    /// Clean the expired memory.
    #[allow(dead_code)]
    pub fn clean(&self) {
        let heap = unsafe {
            self.pool
                .as_ptr()
                .as_mut()
                .expect("MemoryPool is not unique")
        };
        let mut maybe_free = Vec::new();
        while let Some(memory) = heap.peek() {
            if memory.reference_count() > 1 {
                // can't free the memory
                break;
            }
            if let Some(memory) = heap.pop() {
                self.sub_len();
                maybe_free.push(memory);
            }
        }
        for memory in maybe_free {
            if self.min_count() < self.len()
                && now() <= memory.create_time().saturating_add(self.keep_alive_time())
            {
                // free the memory
                continue;
            }
            heap.push(memory);
            self.add_len();
        }
    }
}
