use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NANOS_PER_SEC: u64 = 1_000_000_000;

// get the current wall clock in ns
#[inline]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
        .as_nanos() as u64
}

#[inline]
pub fn dur_to_ns(dur: Duration) -> u64 {
    // Note that a duration is a (u64, u32) (seconds, nanoseconds) pair
    dur.as_secs()
        .saturating_mul(NANOS_PER_SEC)
        .saturating_add(u64::from(dur.subsec_nanos()))
}

pub fn get_timeout_time(dur: Duration) -> u64 {
    add_timeout_time(dur_to_ns(dur))
}

pub fn add_timeout_time(time: u64) -> u64 {
    let now = now();
    match now.checked_add(time) {
        Some(time) => time,
        //处理溢出
        None => u64::MAX,
    }
}

mod generic;

pub use generic::*;

mod typed;

pub use typed::*;

#[cfg(test)]
mod tests {
    use crate::now;

    #[test]
    fn test() {
        println!("{}", now());
    }
}
