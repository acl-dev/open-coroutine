use crate::join::JoinHandle;
use open_coroutine_core::coroutine::suspender::Suspender;
use open_coroutine_core::event_loop::UserFunc;
use std::ffi::c_void;

#[allow(improper_ctypes)]
extern "C" {
    fn coroutine_crate(f: UserFunc, param: usize, stack_size: usize) -> JoinHandle;
}

#[allow(dead_code)]
pub fn co<F, P: 'static, R: 'static>(f: F, param: P, stack_size: usize) -> JoinHandle
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
            (result as *mut R).cast::<c_void>() as usize
        }
    }
    let inner = Box::leak(Box::new((f, param)));
    unsafe {
        coroutine_crate(
            co_main::<F, P, R>,
            (inner as *mut (F, P)).cast::<c_void>() as usize,
            stack_size,
        )
    }
}

#[macro_export]
macro_rules! co {
    ( $f: expr , $param:expr $(,)? ) => {{
        $crate::coroutine::co(
            $f,
            $param,
            //min stack size for backtrace
            64 * 1024,
        )
    }};
    ( $f: expr , $param:expr ,$stack_size: expr $(,)?) => {{
        $crate::coroutine::co($f, $param, $stack_size)
    }};
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    #[test]
    fn simplest() -> std::io::Result<()> {
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::Builder::new()
            .name("test_join".to_string())
            .spawn(move || {
                let handler1 = co!(
                    |_, input| {
                        println!("[coroutine1] launched with {}", input);
                        input
                    },
                    1,
                    1024 * 1024,
                );
                let handler2 = co!(
                    |_, input| {
                        println!("[coroutine2] launched with {}", input);
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
