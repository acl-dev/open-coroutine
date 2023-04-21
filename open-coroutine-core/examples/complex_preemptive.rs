use open_coroutine_core::{Scheduler, Yielder};
use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn null() -> &'static mut c_void {
    unsafe { std::mem::transmute(10usize) }
}

fn main() -> std::io::Result<()> {
    static mut COMPLEX_TEST_FLAG: bool = true;
    static mut COMPLEX_TEST_FLAG2: bool = true;
    let pair = Arc::new((Mutex::new(true), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let handler = std::thread::spawn(move || {
        let mut scheduler = Scheduler::new();

        extern "C" fn f1(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("coroutine1 launched");
            unsafe {
                while COMPLEX_TEST_FLAG {
                    println!("loop1");
                    let _ = libc::usleep(10_000);
                }
            }
            println!("loop1 end");
            null()
        }
        scheduler.submit(f1, null(), 4096).expect("submit failed !");

        extern "C" fn f2(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("coroutine2 launched");
            unsafe {
                while COMPLEX_TEST_FLAG2 {
                    println!("loop2");
                    let _ = libc::usleep(10_000);
                }
            }
            println!("loop2 end");
            unsafe { COMPLEX_TEST_FLAG = false };
            null()
        }
        scheduler.submit(f2, null(), 4096).expect("submit failed !");

        extern "C" fn f3(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("coroutine3 launched");
            unsafe { COMPLEX_TEST_FLAG2 = false };
            null()
        }
        scheduler.submit(f3, null(), 4096).expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");

        let (lock, cvar) = &*pair2;
        let mut pending = lock.lock().unwrap();
        *pending = false;
        // notify the condvar that the value has changed.
        cvar.notify_one();
    });

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
            assert!(!COMPLEX_TEST_FLAG);
        }
        Ok(())
    }
}
