use open_coroutine_core::coroutine::suspender::Suspender;
use open_coroutine_core::net::event_loop::core::EventLoop;
use open_coroutine_core::net::event_loop::UserFunc;
use std::cmp::Ordering;
use std::ffi::{c_char, c_void};
use std::io::{Error, ErrorKind};
use std::time::Duration;

#[allow(improper_ctypes)]
extern "C" {
    fn task_crate(f: UserFunc, param: usize) -> JoinHandle;

    fn task_join(handle: JoinHandle) -> libc::c_long;

    fn task_timeout_join(handle: &JoinHandle, ns_time: u64) -> libc::c_long;
}

pub fn task<F, P: 'static, R: 'static>(f: F, param: P) -> JoinHandle
where
    F: FnOnce(*const Suspender<(), ()>, P) -> R + Copy,
{
    extern "C" fn co_main<F, P: 'static, R: 'static>(
        suspender: *const Suspender<(), ()>,
        input: usize,
    ) -> usize
    where
        F: FnOnce(*const Suspender<(), ()>, P) -> R + Copy,
    {
        unsafe {
            let ptr = &mut *((input as *mut c_void).cast::<(F, P)>());
            let data = std::ptr::read_unaligned(ptr);
            let result: &'static mut R = Box::leak(Box::new((data.0)(suspender, data.1)));
            std::ptr::from_mut::<R>(result).cast::<c_void>() as usize
        }
    }
    let inner = Box::leak(Box::new((f, param)));
    unsafe {
        task_crate(
            co_main::<F, P, R>,
            std::ptr::from_mut::<(F, P)>(inner).cast::<c_void>() as usize,
        )
    }
}

#[macro_export]
macro_rules! task {
    ( $f: expr , $param:expr $(,)? ) => {
        $crate::task::task($f, $param)
    };
}

#[repr(C)]
#[derive(Debug)]
pub struct JoinHandle(*const EventLoop, *const c_char);

impl JoinHandle {
    #[allow(clippy::cast_possible_truncation)]
    pub fn timeout_join<R>(&self, dur: Duration) -> std::io::Result<Option<R>> {
        unsafe {
            let ptr = task_timeout_join(self, dur.as_nanos() as u64);
            match ptr.cmp(&0) {
                Ordering::Less => Err(Error::new(ErrorKind::Other, "timeout join failed")),
                Ordering::Equal => Ok(None),
                Ordering::Greater => Ok(Some(std::ptr::read_unaligned(ptr as *mut R))),
            }
        }
    }

    pub fn join<R>(self) -> std::io::Result<Option<R>> {
        unsafe {
            let ptr = task_join(self);
            match ptr.cmp(&0) {
                Ordering::Less => Err(Error::new(ErrorKind::Other, "join failed")),
                Ordering::Equal => Ok(None),
                Ordering::Greater => Ok(Some(std::ptr::read_unaligned(ptr as *mut R))),
            }
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    #[test]
    fn task_simplest() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let handler1 = task!(
                    |_, input| {
                        println!("[task1] launched with {}", input);
                        input
                    },
                    1,
                );
                let handler2 = task!(
                    |_, input| {
                        println!("[task2] launched with {}", input);
                        input
                    },
                    "hello",
                );
                unsafe {
                    assert_eq!(1, handler1.join().unwrap().unwrap());
                    assert_eq!("hello", &*handler2.join::<*mut str>().unwrap().unwrap());
                }

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
}
