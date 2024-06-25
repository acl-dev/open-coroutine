use crate::common::JoinHandler;
use crate::net::event_loop::core::EventLoop;
use std::ffi::{c_char, CStr, CString};
use std::io::{Error, ErrorKind};
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub struct CoJoinHandle(*const EventLoop, *const c_char);

impl JoinHandler<EventLoop> for CoJoinHandle {
    fn new(event_loop: *const EventLoop, name: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(name).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        CoJoinHandle(event_loop, cstr.as_ptr())
    }

    fn get_name(&self) -> std::io::Result<&str> {
        unsafe { CStr::from_ptr(self.1) }
            .to_str()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid task name"))
    }

    fn timeout_at_join(&self, timeout_time: u64) -> std::io::Result<Result<Option<usize>, &str>> {
        let name = self.get_name()?;
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid task name"));
        }
        let event_loop = unsafe { &*self.0 };
        loop {
            let left_time = timeout_time
                .saturating_sub(open_coroutine_timer::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return Err(Error::new(ErrorKind::TimedOut, "timeout"));
            }
            event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
            if let Some(r) = event_loop.try_get_co_result(name) {
                return Ok(r);
            }
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct TaskJoinHandle(*const EventLoop, *const c_char);

impl JoinHandler<EventLoop> for TaskJoinHandle {
    fn new(event_loop: *const EventLoop, name: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(name).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        TaskJoinHandle(event_loop, cstr.as_ptr())
    }

    fn get_name(&self) -> std::io::Result<&str> {
        unsafe { CStr::from_ptr(self.1) }
            .to_str()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid task name"))
    }

    fn timeout_at_join(&self, timeout_time: u64) -> std::io::Result<Result<Option<usize>, &str>> {
        let name = self.get_name()?;
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid task name"));
        }
        let event_loop = unsafe { &*self.0 };
        loop {
            let left_time = timeout_time
                .saturating_sub(open_coroutine_timer::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return Err(Error::new(ErrorKind::TimedOut, "timeout"));
            }
            event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
            if let Some(r) = event_loop.try_get_task_result(name) {
                return Ok(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Condvar, Mutex};

    #[test]
    fn co_join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle1 = event_loop
                    .submit_co(
                        |_, _| {
                            println!("[coroutine1] launched");
                            Some(3)
                        },
                        None,
                    )
                    .unwrap();
                let handle2 = event_loop
                    .submit_co(
                        |_, _| {
                            println!("[coroutine2] launched");
                            Some(4)
                        },
                        None,
                    )
                    .unwrap();
                assert_eq!(handle1.join().unwrap().unwrap(), Some(3));
                assert_eq!(handle2.join().unwrap().unwrap(), Some(4));

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
            Err(Error::new(ErrorKind::Other, "join failed"))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }

    #[test]
    fn co_timed_join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_timed_join".to_string())
            .spawn(move || {
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle = event_loop
                    .submit_co(
                        |_, _| {
                            println!("[coroutine3] launched");
                            Some(5)
                        },
                        None,
                    )
                    .unwrap();
                let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
                assert_eq!(error.kind(), ErrorKind::TimedOut);
                assert_eq!(
                    handle
                        .timeout_join(Duration::from_secs(1))
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
            Err(Error::new(ErrorKind::Other, "timed join failed"))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }

    #[test]
    fn task_join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle1 = event_loop.submit(
                    |_, _| {
                        println!("[task1] launched");
                        Some(3)
                    },
                    None,
                );
                let handle2 = event_loop.submit(
                    |_, _| {
                        println!("[task2] launched");
                        Some(4)
                    },
                    None,
                );
                assert_eq!(handle1.join().unwrap().unwrap(), Some(3));
                assert_eq!(handle2.join().unwrap().unwrap(), Some(4));

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
            Err(Error::new(ErrorKind::Other, "join failed"))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }

    #[test]
    fn task_timed_join_test() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_timed_join".to_string())
            .spawn(move || {
                let event_loop = EventLoop::new(0, 0, 0, 1, 0).expect("init event loop failed!");
                let handle = event_loop.submit(
                    |_, _| {
                        println!("[task3] launched");
                        Some(5)
                    },
                    None,
                );
                let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
                assert_eq!(error.kind(), ErrorKind::TimedOut);
                assert_eq!(
                    handle
                        .timeout_join(Duration::from_secs(1))
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
            Err(Error::new(ErrorKind::Other, "timed join failed"))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }
}
