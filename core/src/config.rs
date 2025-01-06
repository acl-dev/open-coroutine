use crate::common::constants::{cpu_count, DEFAULT_STACK_SIZE};

#[repr(C)]
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct Config {
    event_loop_size: usize,
    stack_size: usize,
    min_size: usize,
    max_size: usize,
    keep_alive_time: u64,
    min_memory_count: usize,
    memory_keep_alive_time: u64,
    hook: bool,
}

impl Config {
    #[must_use]
    pub fn single() -> Self {
        Self::new(1, DEFAULT_STACK_SIZE, 0, 65536, 0, 0, 0, true)
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_loop_size: usize,
        stack_size: usize,
        min_size: usize,
        max_size: usize,
        keep_alive_time: u64,
        min_memory_count: usize,
        memory_keep_alive_time: u64,
        hook: bool,
    ) -> Self {
        Self {
            event_loop_size,
            stack_size,
            min_size,
            max_size,
            keep_alive_time,
            min_memory_count,
            memory_keep_alive_time,
            hook,
        }
    }

    #[must_use]
    pub fn event_loop_size(&self) -> usize {
        self.event_loop_size
    }

    #[must_use]
    pub fn stack_size(&self) -> usize {
        self.stack_size
    }

    #[must_use]
    pub fn min_size(&self) -> usize {
        self.min_size
    }

    #[must_use]
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    #[must_use]
    pub fn keep_alive_time(&self) -> u64 {
        self.keep_alive_time
    }

    #[must_use]
    pub fn min_memory_count(&self) -> usize {
        self.min_memory_count
    }

    #[must_use]
    pub fn memory_keep_alive_time(&self) -> u64 {
        self.memory_keep_alive_time
    }

    #[must_use]
    pub fn hook(&self) -> bool {
        self.hook
    }

    pub fn set_event_loop_size(&mut self, event_loop_size: usize) -> &mut Self {
        assert!(
            event_loop_size > 0,
            "event_loop_size must be greater than 0"
        );
        self.event_loop_size = event_loop_size;
        self
    }

    pub fn set_stack_size(&mut self, stack_size: usize) -> &mut Self {
        assert!(stack_size > 0, "stack_size must be greater than 0");
        self.stack_size = stack_size;
        self
    }

    pub fn set_min_size(&mut self, min_size: usize) -> &mut Self {
        self.min_size = min_size;
        self
    }

    pub fn set_max_size(&mut self, max_size: usize) -> &mut Self {
        assert!(max_size > 0, "max_size must be greater than 0");
        assert!(
            max_size >= self.min_size,
            "max_size must be greater than or equal to min_size"
        );
        self.max_size = max_size;
        self
    }

    pub fn set_keep_alive_time(&mut self, keep_alive_time: u64) -> &mut Self {
        self.keep_alive_time = keep_alive_time;
        self
    }

    pub fn set_min_memory_count(&mut self, min_memory_count: usize) -> &mut Self {
        self.min_memory_count = min_memory_count;
        self
    }

    pub fn set_memory_keep_alive_time(&mut self, memory_keep_alive_time: u64) -> &mut Self {
        self.memory_keep_alive_time = memory_keep_alive_time;
        self
    }

    pub fn set_hook(&mut self, hook: bool) -> &mut Self {
        self.hook = hook;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(cpu_count(), DEFAULT_STACK_SIZE, 0, 65536, 0, 0, 0, true)
    }
}
