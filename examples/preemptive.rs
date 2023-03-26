use open_coroutine_core_v2::scheduler::Scheduler;
use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn result(result: usize) -> &'static mut c_void {
    unsafe { std::mem::transmute(result) }
}

fn main() -> std::io::Result<()> {
    static mut TEST_FLAG1: bool = true;
    static mut TEST_FLAG2: bool = true;
    let pair = Arc::new((Mutex::new(true), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let handler = std::thread::spawn(move || {
        let scheduler = Scheduler::new();
        let _ = scheduler.submit(|_| async move {
            println!("coroutine1 launched");
            while unsafe { TEST_FLAG1 } {
                println!("loop1");
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("loop1 end");
            unsafe { TEST_FLAG2 = false };
            result(1)
        });
        let _ = scheduler.submit(|_| async move {
            println!("coroutine2 launched");
            while unsafe { TEST_FLAG2 } {
                println!("loop2");
                std::thread::sleep(Duration::from_millis(10));
            }
            println!("loop2 end");
            result(2)
        });
        let _ = scheduler.submit(|_| async move {
            println!("coroutine3 launched");
            unsafe { TEST_FLAG1 = false };
            result(3)
        });
        scheduler.try_schedule();

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
            assert!(!TEST_FLAG1);
        }
        Ok(())
    }
}
