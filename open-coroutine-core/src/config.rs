use once_cell::sync::Lazy;

static mut CONFIG: Lazy<Config> = Lazy::new(Config::default);

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Config {
    event_loop_size: usize,
    stack_size: usize,
    min_size: usize,
    max_size: usize,
    keep_alive_time: u64,
}

impl Config {
    pub fn get_instance() -> &'static mut Config {
        unsafe { &mut CONFIG }
    }

    #[must_use]
    pub fn get_event_loop_size(&self) -> usize {
        self.event_loop_size
    }

    #[must_use]
    pub fn get_stack_size(&self) -> usize {
        self.stack_size
    }

    #[must_use]
    pub fn get_min_size(&self) -> usize {
        self.min_size
    }

    #[must_use]
    pub fn get_max_size(&self) -> usize {
        self.max_size
    }

    #[must_use]
    pub fn get_keep_alive_time(&self) -> u64 {
        self.keep_alive_time
    }

    pub fn set_event_loop_size(&mut self, event_loop_size: usize) -> &mut Self {
        assert!(
            event_loop_size >= 2,
            "event_loop_size must be greater than or equal to 2"
        );
        self.event_loop_size = event_loop_size;
        self
    }

    pub fn set_stack_size(&mut self, stack_size: usize) -> &mut Self {
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            event_loop_size: num_cpus::get(),
            stack_size: crate::coroutine::default_stack_size(),
            min_size: 0,
            max_size: 65536,
            keep_alive_time: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        _ = Config::get_instance()
            .set_event_loop_size(2)
            .set_stack_size(4096)
            .set_min_size(256)
            .set_max_size(256)
            .set_keep_alive_time(0);
        unsafe {
            assert_eq!(2, CONFIG.event_loop_size);
            assert_eq!(4096, CONFIG.stack_size);
            assert_eq!(256, CONFIG.min_size);
            assert_eq!(256, CONFIG.max_size);
            assert_eq!(0, CONFIG.keep_alive_time);
        }
    }
}
