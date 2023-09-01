use crate::event_loop::core::EventLoop;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(*const EventLoop, Arc<(Mutex<Option<usize>>, Condvar)>);

impl JoinHandle {
    pub(crate) fn new(
        event_loop: *const EventLoop,
        pair: Arc<(Mutex<Option<usize>>, Condvar)>,
    ) -> Self {
        JoinHandle(event_loop, pair)
    }

    #[must_use]
    pub fn error() -> Self {
        JoinHandle::new(
            std::ptr::null(),
            Arc::new((Mutex::new(None), Condvar::new())),
        )
    }

    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Option<usize>> {
        self.timeout_at_join(open_coroutine_timer::get_timeout_time(dur))
    }

    pub fn timeout_at_join(&self, timeout_time: u64) -> std::io::Result<Option<usize>> {
        if self.0.is_null() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "can't join"));
        }
        let (lock, cvar) = &*self.1;
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_nanos(timeout_time.saturating_sub(open_coroutine_timer::now())),
                |&mut pending| pending.is_none(),
            )
            .unwrap();
        if result.1.timed_out() {
            Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))
        } else {
            Ok(*result.0)
        }
    }

    pub fn join(self) -> std::io::Result<Option<usize>> {
        if self.0.is_null() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "can't join"));
        }
        let (lock, cvar) = &*self.1;
        let result = cvar
            .wait_while(lock.lock().unwrap(), |&mut pending| pending.is_none())
            .unwrap();
        Ok(*result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_loop::EventLoops;
    use std::sync::{Arc, Condvar, Mutex};

    #[test]
    fn join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let handle1 = EventLoops::submit(|_, _| {
                    println!("[coroutine1] launched");
                    3
                });
                let handle2 = EventLoops::submit(|_, _| {
                    println!("[coroutine2] launched");
                    4
                });
                assert_eq!(handle1.join().unwrap().unwrap(), 3);
                assert_eq!(handle2.join().unwrap().unwrap(), 4);

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
                "join failed",
            ))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }

    #[test]
    fn timed_join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_timed_join".to_string())
            .spawn(move || {
                let handle = EventLoops::submit(|_, _| {
                    println!("[coroutine3] launched");
                    5
                });
                let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
                assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                assert_eq!(
                    handle
                        .timeout_join(Duration::from_secs(1))
                        .unwrap()
                        .unwrap(),
                    5
                );

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
                "timed join failed",
            ))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }
}
