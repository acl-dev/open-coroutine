use crate::common::now;
use crate::config::Config;
use corosensei::stack::{DefaultStack, Stack, StackPointer};
use once_cell::sync::OnceCell;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize};

pub(crate) struct PooledStack {
    stack_size: usize,
    stack: Rc<UnsafeCell<DefaultStack>>,
    create_time: u64,
}

impl Deref for PooledStack {
    type Target = DefaultStack;

    fn deref(&self) -> &DefaultStack {
        unsafe {
            self.stack
                .deref()
                .get()
                .as_ref()
                .expect("PooledStack is not unique")
        }
    }
}

impl DerefMut for PooledStack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.stack
                .deref()
                .get()
                .as_mut()
                .expect("PooledStack is not unique")
        }
    }
}

impl Clone for PooledStack {
    fn clone(&self) -> Self {
        Self {
            stack_size: self.stack_size,
            stack: self.stack.clone(),
            create_time: self.create_time,
        }
    }
}

impl PartialEq<Self> for PooledStack {
    fn eq(&self, other: &Self) -> bool {
        Rc::strong_count(&other.stack).eq(&Rc::strong_count(&self.stack))
    }
}

impl Eq for PooledStack {}

impl PartialOrd<Self> for PooledStack {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PooledStack {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap defaults to a large top heap, but we need a small top heap
        match Rc::strong_count(&other.stack).cmp(&Rc::strong_count(&self.stack)) {
            Ordering::Less => Ordering::Less,
            Ordering::Equal => match other.stack_size.cmp(&self.stack_size) {
                Ordering::Less => Ordering::Less,
                Ordering::Equal => other.create_time.cmp(&self.create_time),
                Ordering::Greater => Ordering::Greater,
            },
            Ordering::Greater => Ordering::Greater,
        }
    }
}

unsafe impl Stack for PooledStack {
    #[inline]
    fn base(&self) -> StackPointer {
        self.deref().base()
    }

    #[inline]
    fn limit(&self) -> StackPointer {
        self.deref().limit()
    }

    #[cfg(windows)]
    #[inline]
    fn teb_fields(&self) -> corosensei::stack::StackTebFields {
        self.deref().teb_fields()
    }

    #[cfg(windows)]
    #[inline]
    fn update_teb_fields(&mut self, stack_limit: usize, guaranteed_stack_bytes: usize) {
        self.deref_mut()
            .update_teb_fields(stack_limit, guaranteed_stack_bytes);
    }
}

impl PooledStack {
    pub(crate) fn new(stack_size: usize) -> std::io::Result<Self> {
        Ok(Self {
            stack_size,
            stack: Rc::new(UnsafeCell::new(DefaultStack::new(stack_size)?)),
            create_time: now(),
        })
    }

    /// This function must be called after a stack has finished running a coroutine
    /// so that the `StackLimit` and `GuaranteedStackBytes` fields from the TEB can
    /// be updated in the stack. This is necessary if the stack is reused for
    /// another coroutine.
    #[inline]
    #[cfg(windows)]
    pub(crate) fn update_stack_teb_fields(&mut self) {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "x86_64")] {
                type StackWord = u64;
            } else if #[cfg(target_arch = "x86")] {
                type StackWord = u32;
            }
        }
        let base = self.base().get() as *const StackWord;
        unsafe {
            let stack_limit = usize::try_from(*base.sub(1)).expect("stack limit overflow");
            let guaranteed_stack_bytes =
                usize::try_from(*base.sub(2)).expect("guaranteed stack bytes overflow");
            self.update_teb_fields(stack_limit, guaranteed_stack_bytes);
        }
    }
}

static STACK_POOL: OnceCell<MemoryPool> = OnceCell::new();

/// A memory pool for reusing stacks.
#[derive(educe::Educe)]
#[educe(Debug)]
pub struct MemoryPool {
    #[educe(Debug(ignore))]
    pool: UnsafeCell<BinaryHeap<PooledStack>>,
    len: AtomicUsize,
    //最小内存数，即核心内存数
    min_count: AtomicUsize,
    //非核心内存的最大存活时间，单位ns
    keep_alive_time: AtomicU64,
}

unsafe impl Send for MemoryPool {}

unsafe impl Sync for MemoryPool {}

impl Default for MemoryPool {
    fn default() -> Self {
        Self::new(0, 10_000_000_000)
    }
}

impl MemoryPool {
    /// Init the `MemoryPool`.
    pub fn init(config: &Config) -> Result<(), MemoryPool> {
        STACK_POOL.set(MemoryPool::new(
            config.min_memory_count(),
            config.memory_keep_alive_time(),
        ))
    }

    pub(crate) fn get_instance<'m>() -> &'m Self {
        STACK_POOL.get_or_init(MemoryPool::default)
    }

    pub(crate) fn new(min_count: usize, keep_alive_time: u64) -> Self {
        Self {
            pool: UnsafeCell::new(BinaryHeap::default()),
            len: AtomicUsize::default(),
            min_count: AtomicUsize::new(min_count),
            keep_alive_time: AtomicU64::new(keep_alive_time),
        }
    }

    pub(crate) fn allocate(&self, stack_size: usize) -> std::io::Result<PooledStack> {
        let heap = unsafe { self.pool.get().as_mut().expect("StackPool is not unique") };
        // find min stack
        let mut not_use = Vec::new();
        while let Some(stack) = heap.peek() {
            if Rc::strong_count(&stack.stack) > 1 {
                // can't use the stack
                break;
            }
            #[allow(unused_mut)]
            if let Some(mut stack) = heap.pop() {
                self.sub_len();
                if stack_size <= stack.stack_size {
                    for s in not_use {
                        heap.push(s);
                        self.add_len();
                    }
                    heap.push(stack.clone());
                    self.add_len();
                    #[cfg(windows)]
                    stack.update_stack_teb_fields();
                    return Ok(stack);
                }
                if self.min_count() < self.len()
                    && now() <= stack.create_time.saturating_add(self.keep_alive_time())
                {
                    // clean the expired stack
                    continue;
                }
                not_use.push(stack);
            }
        }
        let stack = PooledStack::new(stack_size)?;
        heap.push(stack.clone());
        self.add_len();
        Ok(stack)
    }

    #[allow(dead_code)]
    pub(crate) fn set_keep_alive_time(&self, keep_alive_time: u64) {
        self.keep_alive_time
            .store(keep_alive_time, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn keep_alive_time(&self) -> u64 {
        self.keep_alive_time
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[allow(dead_code)]
    pub(crate) fn set_min_count(&self, min_count: usize) {
        self.min_count
            .store(min_count, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn min_count(&self) -> usize {
        self.min_count.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn len(&self) -> usize {
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

    /// Clean the expired stack.
    #[allow(dead_code)]
    pub(crate) fn clean(&self) {
        let heap = unsafe { self.pool.get().as_mut().expect("StackPool is not unique") };
        let mut maybe_free = Vec::new();
        while let Some(stack) = heap.peek() {
            if Rc::strong_count(&stack.stack) > 1 {
                // can't free the stack
                break;
            }
            if let Some(stack) = heap.pop() {
                self.sub_len();
                maybe_free.push(stack);
            }
        }
        for stack in maybe_free {
            if self.min_count() < self.len()
                && now() <= stack.create_time.saturating_add(self.keep_alive_time())
            {
                // free the stack
                continue;
            }
            heap.push(stack);
            self.add_len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::DEFAULT_STACK_SIZE;

    #[test]
    fn test_stack_pool() -> std::io::Result<()> {
        let pool = MemoryPool::default();
        let stack = pool.allocate(DEFAULT_STACK_SIZE)?;
        assert_eq!(Rc::strong_count(&stack.stack), 2);
        drop(stack);
        let stack = pool.allocate(DEFAULT_STACK_SIZE)?;
        assert_eq!(Rc::strong_count(&stack.stack), 2);
        assert_eq!(pool.len(), 1);
        _ = pool.allocate(DEFAULT_STACK_SIZE)?;
        assert_eq!(pool.len(), 2);
        Ok(())
    }
}
