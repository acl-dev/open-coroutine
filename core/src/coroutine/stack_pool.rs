use corosensei::stack::{DefaultStack, Stack, StackPointer};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::Deref;
use std::sync::Arc;

pub(crate) struct PooledStack(Arc<DefaultStack>);

impl Deref for PooledStack {
    type Target = Arc<DefaultStack>;

    fn deref(&self) -> &Arc<DefaultStack> {
        &self.0
    }
}

impl PartialEq<Self> for PooledStack {
    fn eq(&self, other: &Self) -> bool {
        Arc::strong_count(other).eq(&Arc::strong_count(self))
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
        Arc::strong_count(other).cmp(&Arc::strong_count(self))
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
        self.deref()
            .update_teb_fields(stack_limit, guaranteed_stack_bytes)
    }
}

#[derive(Default)]
pub(crate) struct StackPool(Arc<RefCell<BinaryHeap<PooledStack>>>);

unsafe impl Send for StackPool {}

unsafe impl Sync for StackPool {}

impl StackPool {
    pub(crate) fn get_stack(&self, stack_size: usize) -> std::io::Result<PooledStack> {
        loop {
            if let Ok(mut heap) = self.0.try_borrow_mut() {
                if let Some(stack) = heap.peek() {
                    if Arc::strong_count(stack) == 1 {
                        return Ok(PooledStack(stack.deref().clone()));
                    }
                }
                let stack = Arc::new(DefaultStack::new(stack_size)?);
                heap.push(PooledStack(stack.clone()));
                return Ok(PooledStack(stack));
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.0.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::constants::DEFAULT_STACK_SIZE;

    #[test]
    fn test_stack_pool() -> std::io::Result<()> {
        let pool = StackPool::default();
        let stack = pool.get_stack(DEFAULT_STACK_SIZE)?;
        assert_eq!(Arc::strong_count(&stack), 2);
        drop(stack);
        let stack = pool.get_stack(DEFAULT_STACK_SIZE)?;
        assert_eq!(Arc::strong_count(&stack), 2);
        assert_eq!(pool.len(), 1);
        _ = pool.get_stack(DEFAULT_STACK_SIZE)?;
        assert_eq!(pool.len(), 2);
        Ok(())
    }
}
