use crate::net::event_loop::core::EventLoop;
use crate::pool::has::HasCoroutinePool;
use std::ffi::{c_char, CStr, CString};
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandleImpl(*const EventLoop, *const c_char);

impl JoinHandleImpl {
    pub(crate) fn new(event_loop: *const EventLoop, string: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(string).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandleImpl(event_loop, cstr.as_ptr())
    }

    #[must_use]
    pub fn error() -> Self {
        JoinHandleImpl::new(std::ptr::null(), "")
    }

    pub fn timeout_join(
        &self,
        dur: Duration,
    ) -> std::io::Result<Option<Result<Option<usize>, &str>>> {
        self.timeout_at_join(open_coroutine_timer::get_timeout_time(dur))
    }

    pub fn timeout_at_join(
        &self,
        timeout_time: u64,
    ) -> std::io::Result<Option<Result<Option<usize>, &str>>> {
        match unsafe { CStr::from_ptr(self.1) }.to_str() {
            Ok(co_name) => {
                if co_name.is_empty() {
                    return Ok(None);
                }
                let event_loop = unsafe { &*self.0 };
                loop {
                    let left_time = timeout_time
                        .saturating_sub(open_coroutine_timer::now())
                        .min(10_000_000);
                    if left_time == 0 {
                        //timeout
                        return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
                    }
                    event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
                    if let Some((_, result)) = event_loop.try_get_task_result(co_name) {
                        return Ok(Some(result));
                    }
                }
            }
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid coroutine name",
            )),
        }
    }

    pub fn join<'e>(self) -> std::io::Result<Option<Result<Option<usize>, &'e str>>> {
        match unsafe { CStr::from_ptr(self.1) }.to_str() {
            Ok(co_name) => {
                if co_name.is_empty() {
                    return Ok(None);
                }
                let event_loop = unsafe { &*self.0 };
                loop {
                    event_loop.wait_event(Some(Duration::from_millis(10)))?;
                    if let Some((_, result)) = event_loop.try_get_task_result(co_name) {
                        return Ok(Some(result));
                    }
                }
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
                let handle1 = event_loop.submit(
                    |_, _| {
                        println!("[coroutine1] launched");
                        Some(3)
                    },
                    None,
                );
                let handle2 = event_loop.submit(
                    |_, _| {
                        println!("[coroutine2] launched");
                        Some(4)
                    },
                    None,
                );
                assert_eq!(handle1.join().unwrap().unwrap().unwrap(), Some(3));
                assert_eq!(handle2.join().unwrap().unwrap().unwrap(), Some(4));

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
                let handle = event_loop.submit(
                    |_, _| {
                        println!("[coroutine3] launched");
                        Some(5)
                    },
                    None,
                );
                let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
                assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                assert_eq!(
                    handle
                        .timeout_join(Duration::from_secs(1))
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                    Some(5)
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
