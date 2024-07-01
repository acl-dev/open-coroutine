use crate::common::JoinHandler;
use crate::pool::CoroutinePool;
use std::ffi::{c_char, CStr, CString};
use std::io::{Error, ErrorKind};
use std::time::Duration;

#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle<'j>(*const CoroutinePool<'j>, *const c_char);

impl<'j> JoinHandler<CoroutinePool<'j>> for JoinHandle<'j> {
    fn new(pool: *const CoroutinePool<'j>, name: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(name).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandle(pool, cstr.as_ptr())
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
        let pool = unsafe { &*self.0 };
        pool.wait_result(
            name,
            Duration::from_nanos(timeout_time.saturating_sub(open_coroutine_timer::now())),
        )
        .map(|r| r.expect("result is None !"))
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
                let pool = CoroutinePool::default();
                let handle1 = pool.submit(
                    None,
                    |_, _| {
                        println!("[coroutine1] launched");
                        Some(3)
                    },
                    None,
                );
                let handle2 = pool.submit(
                    None,
                    |_, _| {
                        println!("[coroutine2] launched");
                        Some(4)
                    },
                    None,
                );
                pool.try_schedule_task().unwrap();
                assert_eq!(handle1.join().unwrap().unwrap().unwrap(), 3);
                assert_eq!(handle2.join().unwrap().unwrap().unwrap(), 4);

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
            Err(Error::new(ErrorKind::TimedOut, "join failed"))
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
                let pool = CoroutinePool::default();
                let handle = pool.submit(
                    None,
                    |_, _| {
                        println!("[coroutine3] launched");
                        Some(5)
                    },
                    None,
                );
                let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
                assert_eq!(error.kind(), ErrorKind::TimedOut);
                pool.try_schedule_task().unwrap();
                assert_eq!(
                    handle
                        .timeout_join(Duration::from_secs(1))
                        .unwrap()
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
            Err(Error::new(ErrorKind::TimedOut, "timed join failed"))
        } else {
            handler.join().unwrap();
            Ok(())
        }
    }
}
