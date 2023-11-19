use super::*;
use crate::common::Named;

#[test]
fn test_simple() {
    #[derive(Debug)]
    struct SleepBlocker {}

    impl Named for SleepBlocker {
        fn get_name(&self) -> &str {
            "SleepBlocker"
        }
    }
    impl Blocker for SleepBlocker {
        fn block(&self, time: Duration) {
            std::thread::sleep(time)
        }
    }

    let pool = Box::leak(Box::new(CoroutinePoolImpl::new(
        Uuid::new_v4().to_string(),
        0,
        0,
        0,
        2,
        0,
        SleepBlocker {},
    )));
    _ = pool.submit(
        None,
        |_, _| {
            println!("1");
            None
        },
        None,
    );
    _ = pool.submit(
        None,
        |_, _| {
            println!("2");
            None
        },
        None,
    );
    _ = pool.try_timed_schedule_task(Duration::from_secs(1));
}
