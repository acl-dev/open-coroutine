use std::os::raw::{c_int, c_void};
use libfiber::fiber::Fiber;
use libfiber::scheduler::{EventMode, Scheduler};

fn main() {
    let env = 1;
    Fiber::new(|fiber, arg| {
        println!("env {}", env);
        match arg {
            Some(arg) => println!("arg:{}", arg as usize),
            None => println!("no param")
        }
    }, Some(2 as *mut c_void), 128 * 1024 * 1024);
    let scheduler = Scheduler::new(EventMode::Kernel);
    scheduler.start();
    scheduler.stop();
    println!("finished !");
}