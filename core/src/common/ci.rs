use std::time::{Duration, Instant};

/// just for CI
pub fn init() {
    let _ = std::thread::spawn(|| {
        // exit after 600 seconds, just for CI
        let sleep_time = Duration::from_secs(600);
        let start_time = Instant::now();
        std::thread::sleep(sleep_time);
        let cost = Instant::now().saturating_duration_since(start_time);
        assert!(cost >= sleep_time, "CI time consumption less than expected");
        std::process::exit(-1);
    });
}
