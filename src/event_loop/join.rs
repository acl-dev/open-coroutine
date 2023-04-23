use crate::event_loop::EventLoop;
use crate::scheduler::Scheduler;
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::Duration;

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(Option<*const EventLoop>, *const c_char);

impl JoinHandle {
    pub(crate) fn new(event_loop: Option<*const EventLoop>, string: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(CString::new(string).unwrap()));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandle(event_loop, cstr.as_ptr())
    }

    #[must_use]
    pub fn error() -> Self {
        JoinHandle::new(None, "")
    }

    pub fn timeout_join(&self, dur: Duration) -> std::io::Result<Option<&'static mut c_void>> {
        self.timeout_at_join(timer_utils::get_timeout_time(dur))
    }

    pub fn timeout_at_join(
        &self,
        timeout_time: u64,
    ) -> std::io::Result<Option<&'static mut c_void>> {
        let co_name = unsafe { CStr::from_ptr(self.1).to_str().unwrap() };
        if co_name.is_empty() {
            return Ok(None);
        }
        let event_loop = unsafe { &*self.0.unwrap() };
        let mut result = Scheduler::get_result(co_name);
        while result.is_none() {
            let left_time = timeout_time
                .saturating_sub(timer_utils::now())
                .min(10_000_000);
            if left_time == 0 {
                //timeout
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
            event_loop.wait_event(Some(Duration::from_nanos(left_time)))?;
            result = Scheduler::get_result(co_name);
        }
        Ok(result.unwrap().get_result())
    }

    pub fn join(self) -> std::io::Result<Option<&'static mut c_void>> {
        let co_name = unsafe { CStr::from_ptr(self.1).to_str().unwrap() };
        if co_name.is_empty() {
            return Ok(None);
        }
        let event_loop = unsafe { &*self.0.unwrap() };
        let mut result = Scheduler::get_result(co_name);
        while result.is_none() {
            event_loop.wait_event(Some(Duration::from_millis(10)))?;
            result = Scheduler::get_result(co_name);
        }
        Ok(result.unwrap().get_result())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(val: usize) -> &'static mut c_void {
        unsafe { std::mem::transmute(val) }
    }

    #[test]
    fn join_test() {
        let event_loop = EventLoop::new().unwrap();
        let handle1 = event_loop
            .submit(|_, _| {
                println!("[coroutine1] launched");
                val(1)
            })
            .expect("submit failed !");
        let handle2 = event_loop
            .submit(|_, _| {
                println!("[coroutine2] launched");
                val(2)
            })
            .expect("submit failed !");
        assert_eq!(handle1.join().unwrap().unwrap() as *mut c_void as usize, 1);
        assert_eq!(handle2.join().unwrap().unwrap() as *mut c_void as usize, 2);
    }

    #[test]
    fn timed_join_test() {
        let event_loop = EventLoop::new().unwrap();
        let handle = event_loop
            .submit(|_, _| {
                println!("[coroutine3] launched");
                val(3)
            })
            .expect("submit failed !");
        let error = handle.timeout_join(Duration::from_nanos(0)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            handle
                .timeout_join(Duration::from_secs(1))
                .unwrap()
                .unwrap() as *mut c_void as usize,
            3
        );
    }
}
