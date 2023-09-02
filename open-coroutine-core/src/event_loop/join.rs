use crate::event_loop::core::EventLoop;
use std::ffi::{c_char, CStr, CString};
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(*const EventLoop, *const c_char);

impl JoinHandle {
    pub(crate) fn new(event_loop: *const EventLoop, string: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(string).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandle(event_loop, cstr.as_ptr())
    }

    #[must_use]
    pub fn error() -> Self {
        JoinHandle::new(std::ptr::null(), "")
    }

    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Option<usize>> {
        self.timeout_at_join(open_coroutine_timer::get_timeout_time(dur))
    }

    pub fn timeout_at_join(&self, timeout_time: u64) -> std::io::Result<Option<usize>> {
        match unsafe { CStr::from_ptr(self.1) }.to_str() {
            Ok(co_name) => {
                if co_name.is_empty() {
                    return Ok(None);
                }
                let event_loop = unsafe { &*self.0 };
                let mut result = EventLoop::get_result(co_name);
                while result.is_none() {
                    let left_time = timeout_time
                        .saturating_sub(open_coroutine_timer::now())
                        .min(10_000_000);
                    if left_time == 0 {
                        //timeout
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
                    }
                    event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
                    result = EventLoop::get_result(co_name);
                }
                Ok(result)
            }
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid coroutine name",
            )),
        }
    }

    pub fn join(self) -> std::io::Result<Option<usize>> {
        match unsafe { CStr::from_ptr(self.1) }.to_str() {
            Ok(co_name) => {
                if co_name.is_empty() {
                    return Ok(None);
                }
                let event_loop = unsafe { &*self.0 };
                let mut result = EventLoop::get_result(co_name);
                while result.is_none() {
                    event_loop.wait_event(Some(Duration::from_millis(10)))?;
                    result = EventLoop::get_result(co_name);
                }
                Ok(result)
            }
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid coroutine name",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Condvar, Mutex};

    #[test]
    fn join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle1 = event_loop.submit(|_, _| {
                    println!("[coroutine1] launched");
                    3
                });
                let handle2 = event_loop.submit(|_, _| {
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
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle = event_loop.submit(|_, _| {
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
