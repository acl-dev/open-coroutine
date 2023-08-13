use crossbeam_utils::atomic::AtomicCell;
use once_cell::sync::Lazy;
use std::fmt::{Debug, Formatter};

static CONFIG: Lazy<Config> = Lazy::new(Config::default);

#[repr(C)]
pub struct Config {
    event_loop_size: AtomicCell<usize>,
    stack_size: AtomicCell<usize>,
    min_size: AtomicCell<usize>,
    max_size: AtomicCell<usize>,
    keep_alive_time: AtomicCell<u64>,
}

impl Config {
    #[must_use]
    pub fn get_instance() -> &'static Config {
        &CONFIG
    }

    #[must_use]
    pub fn get_event_loop_size(&self) -> usize {
        self.event_loop_size.load()
    }

    #[must_use]
    pub fn get_stack_size(&self) -> usize {
        self.stack_size.load()
    }

    #[must_use]
    pub fn get_min_size(&self) -> usize {
        self.min_size.load()
    }

    #[must_use]
    pub fn get_max_size(&self) -> usize {
        self.max_size.load()
    }

    #[must_use]
    pub fn get_keep_alive_time(&self) -> u64 {
        self.keep_alive_time.load()
    }

    pub fn set_event_loop_size(&self, event_loop_size: usize) -> &Self {
        assert!(
            event_loop_size > 1,
            "event_loop_size must be greater than 1"
        );
        self.event_loop_size.store(event_loop_size);
        self
    }

    pub fn set_stack_size(&self, stack_size: usize) -> &Self {
        self.stack_size.store(stack_size);
        self
    }

    pub fn set_min_size(&self, min_size: usize) -> &Self {
        self.min_size.store(min_size);
        self
    }

    pub fn set_max_size(&self, max_size: usize) -> &Self {
        assert!(max_size > 0, "max_size must be greater than 0");
        assert!(
            max_size >= self.min_size.load(),
            "max_size must be greater than or equal to min_size"
        );
        self.max_size.store(max_size);
        self
    }

    pub fn set_keep_alive_time(&self, keep_alive_time: u64) -> &Self {
        self.keep_alive_time.store(keep_alive_time);
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            event_loop_size: AtomicCell::new(num_cpus::get()),
            stack_size: AtomicCell::new(crate::coroutine::default_stack_size()),
            min_size: AtomicCell::new(0),
            max_size: AtomicCell::new(65536),
            keep_alive_time: AtomicCell::new(0),
        }
    }
}

impl Debug for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("event_loop_size", &self.get_event_loop_size())
            .field("stack_size", &self.get_stack_size())
            .field("min_size", &self.get_min_size())
            .field("max_size", &self.get_max_size())
            .field("keep_alive_time", &self.get_keep_alive_time())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn test_config() {
        _ = Config::get_instance()
            .set_event_loop_size(2)
            .set_stack_size(4096)
            .set_min_size(256)
            .set_max_size(256)
            .set_keep_alive_time(0);
        assert_eq!(2, CONFIG.event_loop_size.load());
        assert_eq!(4096, CONFIG.stack_size.load());
        assert_eq!(256, CONFIG.min_size.load());
        assert_eq!(256, CONFIG.max_size.load());
        assert_eq!(0, CONFIG.keep_alive_time.load());
    }
}
