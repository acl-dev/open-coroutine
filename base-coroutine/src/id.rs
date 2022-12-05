use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct IdGenerator {}

static mut COROUTINE_ID: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(1));

static mut SCHEDULER_ID: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(1));

impl IdGenerator {
    fn reset() {
        unsafe {
            COROUTINE_ID.store(1, Ordering::SeqCst);
            SCHEDULER_ID.store(1, Ordering::SeqCst);
        }
    }

    pub fn next_coroutine_id() -> usize {
        unsafe {
            let val = COROUTINE_ID.fetch_add(1, Ordering::SeqCst);
            if val == usize::MAX {
                COROUTINE_ID.store(1, Ordering::SeqCst);
            }
            val
        }
    }

    pub fn next_scheduler_id() -> usize {
        unsafe {
            let val = SCHEDULER_ID.fetch_add(1, Ordering::SeqCst);
            if val == usize::MAX {
                COROUTINE_ID.store(1, Ordering::SeqCst);
            }
            val
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IdGenerator;

    #[test]
    fn test() {
        IdGenerator::reset();
        assert_eq!(1, IdGenerator::next_coroutine_id());
        assert_eq!(2, IdGenerator::next_coroutine_id());
        assert_eq!(3, IdGenerator::next_coroutine_id());
        IdGenerator::reset();
        assert_eq!(1, IdGenerator::next_scheduler_id());
        assert_eq!(2, IdGenerator::next_scheduler_id());
        assert_eq!(3, IdGenerator::next_scheduler_id());
    }
}
