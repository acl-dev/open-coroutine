use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct IdGenerator {}

static mut COROUTINE_ID: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(1));

static mut SCHEDULER_ID: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(1));

impl IdGenerator {
    pub fn next_coroutine_id() -> usize {
        unsafe { COROUTINE_ID.fetch_add(1, Ordering::SeqCst) }
    }

    pub fn next_scheduler_id() -> usize {
        unsafe { SCHEDULER_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

#[cfg(test)]
mod tests {
    use crate::IdGenerator;

    #[test]
    fn test() {
        assert_eq!(1, IdGenerator::next_coroutine_id());
        assert_eq!(2, IdGenerator::next_coroutine_id());
        assert_eq!(3, IdGenerator::next_coroutine_id());

        assert_eq!(1, IdGenerator::next_scheduler_id());
        assert_eq!(2, IdGenerator::next_scheduler_id());
        assert_eq!(3, IdGenerator::next_scheduler_id());
    }
}
