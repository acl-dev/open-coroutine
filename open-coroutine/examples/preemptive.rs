/// outputs:
/// ```
/// coroutine1 launched
/// loop1
/// coroutine2 launched
/// loop2
/// coroutine3 launched
/// loop1
/// loop2 end
/// loop1 end
/// ```
pub fn main() -> std::io::Result<()> {
    cfg_if::cfg_if! {
        if #[cfg(all(unix, feature = "preemptive"))] {
            use open_coroutine_core::scheduler::Scheduler;
            use std::sync::{Arc, Condvar, Mutex};
            use std::time::Duration;

            static mut TEST_FLAG1: bool = true;
            static mut TEST_FLAG2: bool = true;
            let pair = Arc::new((Mutex::new(true), Condvar::new()));
            let pair2 = Arc::clone(&pair);
            let handler = std::thread::Builder::new()
                .name("preemptive".to_string())
                .spawn(move || {
                    let mut scheduler = Scheduler::default();
                    _ = scheduler.submit_co(
                        |_, _| {
                            println!("coroutine1 launched");
                            while unsafe { TEST_FLAG1 } {
                                println!("loop1");
                                _ = unsafe { libc::usleep(10_000) };
                            }
                            println!("loop1 end");
                            None
                        },
                        None,
                        None,
                    );
                    _ = scheduler.submit_co(
                        |_, _| {
                            println!("coroutine2 launched");
                            while unsafe { TEST_FLAG2 } {
                                println!("loop2");
                                _ = unsafe { libc::usleep(10_000) };
                            }
                            println!("loop2 end");
                            unsafe { TEST_FLAG1 = false };
                            None
                        },
                        None,
                        None,
                    );
                    _ = scheduler.submit_co(
                        |_, _| {
                            println!("coroutine3 launched");
                            unsafe { TEST_FLAG2 = false };
                            None
                        },
                        None,
                        None,
                    );
                    scheduler.try_schedule().expect("schedule failed");

                    let (lock, cvar) = &*pair2;
                    let mut pending = lock.lock().unwrap();
                    *pending = false;
                    // notify the condvar that the value has changed.
                    cvar.notify_one();
                })
                .expect("failed to spawn thread");

            // wait for the thread to start up
            let (lock, cvar) = &*pair;
            let result = cvar
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_millis(3000),
                    |&mut pending| pending,
                )
                .unwrap();
            if result.1.timed_out() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "preemptive schedule failed",
                ))
            } else {
                unsafe {
                    handler.join().unwrap();
                    assert!(!TEST_FLAG1);
                }
                Ok(())
            }
        } else {
            println!("please enable preemptive feature");
            Ok(())
        }
    }
}
