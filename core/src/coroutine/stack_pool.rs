use crate::common::memory_pool::{MemoryPool, ReusableMemory};
use crate::common::now;
use crate::config::Config;
use crate::coroutine::StackInfo;
use corosensei::stack::{DefaultStack, Stack, StackPointer};
use once_cell::sync::OnceCell;
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// A wrapper for reusing `DefaultStack`.
pub struct PooledStack {
    stack_size: usize,
    stack: Rc<UnsafeCell<DefaultStack>>,
    create_time: u64,
}

impl Debug for PooledStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledStack")
            .field("stack_size", &self.stack_size)
            .field("stack", &StackInfo::from(self))
            .field("create_time", &self.create_time)
            .finish()
    }
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

unsafe impl ReusableMemory for PooledStack {
    fn new(stack_size: usize) -> std::io::Result<Self> {
        Ok(Self {
            stack_size,
            stack: Rc::new(UnsafeCell::new(DefaultStack::new(stack_size)?)),
            create_time: now(),
        })
    }

    fn reference_count(&self) -> usize {
        Rc::strong_count(&self.stack)
    }

    fn top(&self) -> NonZeroUsize {
        self.base()
    }

    fn bottom(&self) -> NonZeroUsize {
        self.limit()
    }

    fn create_time(&self) -> u64 {
        self.create_time
    }

    #[inline]
    fn on_reuse(&mut self) -> std::io::Result<()> {
        // This function must be called after a stack has finished running a coroutine
        // so that the `StackLimit` and `GuaranteedStackBytes` fields from the TEB can
        // be updated in the stack. This is necessary if the stack is reused for
        // another coroutine.
        cfg_if::cfg_if! {
            if #[cfg(windows)] {
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
        Ok(())
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

static STACK_POOL: OnceCell<StackPool> = OnceCell::new();

/// A memory pool for reusing stacks.
#[derive(Debug, Default)]
pub struct StackPool(MemoryPool<PooledStack>);

impl StackPool {
    /// Init the `MemoryPool`.
    pub fn init(config: &Config) -> Result<(), StackPool> {
        let pool = StackPool::default();
        _ = pool
            .set_min_count(config.min_memory_count())
            .set_keep_alive_time(config.memory_keep_alive_time());
        STACK_POOL.set(pool)
    }

    pub(crate) fn get_instance<'m>() -> &'m Self {
        STACK_POOL.get_or_init(StackPool::default)
    }
}

impl Deref for StackPool {
    type Target = MemoryPool<PooledStack>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::DEFAULT_STACK_SIZE;

    #[test]
    fn test_stack_pool() -> std::io::Result<()> {
        let pool = StackPool::default();
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
