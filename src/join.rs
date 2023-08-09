use open_coroutine_core::event_loop::core::EventLoop;
use std::cmp::Ordering;
use std::ffi::c_char;
use std::io::{Error, ErrorKind};
use std::time::Duration;

#[allow(improper_ctypes)]
extern "C" {
    fn coroutine_join(handle: JoinHandle) -> libc::c_long;

    fn coroutine_timeout_join(handle: &JoinHandle, ns_time: u64) -> libc::c_long;
}

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(*const EventLoop, *const c_char);

impl JoinHandle {
    #[allow(clippy::cast_possible_truncation)]
    pub fn timeout_join<R>(&self, dur: Duration) -> std::io::Result<Option<R>> {
        unsafe {
            let ptr = coroutine_timeout_join(self, dur.as_nanos() as u64);
            match ptr.cmp(&0) {
                Ordering::Less => Err(Error::new(ErrorKind::Other, "timeout join failed")),
                Ordering::Equal => Ok(None),
                Ordering::Greater => Ok(Some(std::ptr::read_unaligned(ptr as *mut R))),
            }
        }
    }

    pub fn join<R>(self) -> std::io::Result<Option<R>> {
        unsafe {
            let ptr = coroutine_join(self);
            match ptr.cmp(&0) {
                Ordering::Less => Err(Error::new(ErrorKind::Other, "join failed")),
                Ordering::Equal => Ok(None),
                Ordering::Greater => Ok(Some(std::ptr::read_unaligned(ptr as *mut R))),
            }
        }
    }
}
