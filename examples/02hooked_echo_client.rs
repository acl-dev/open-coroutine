use std::ffi::c_void;
use std::io::Error;
use std::{mem, thread};
use std::os::raw::c_int;
use libc as c;
use threadpool::ThreadPool;
use libfiber::condition::Condition;
use libfiber::event::Event;
use libfiber::fiber::{current_id, delay, Fiber};
use libfiber::scheduler::{EventMode, Scheduler};

const package_length: usize = 512;
const clients: i32 = 500;
const duration: i64 = 30 * 1000;

fn fiber_request(_: *const c_void, _: Option<*mut c_void>) {
    unsafe {
        let socket = c::socket(c::AF_INET, c::SOCK_STREAM, c::IPPROTO_TCP);
        if socket < 0 {
            eprintln!("last OS error: {:?}", Error::last_os_error());
            return;
        }

        let servaddr = c::sockaddr_in {
            sin_len: 0,
            sin_family: c::AF_INET as u8,
            sin_port: 9898u16.to_be(),
            sin_addr: c::in_addr {
                s_addr: u32::from_be_bytes([127, 0, 0, 1]).to_be()
            },
            sin_zero: mem::zeroed(),
        };

        let result = c::connect(socket, &servaddr as *const c::sockaddr_in as *const c::sockaddr, mem::size_of_val(&servaddr) as u32);
        if result < 0 {
            println!("last OS error: {:?}", Error::last_os_error());
            c::close(socket);
        }
        println!("fiber-{} connect ok !", current_id());
        let msg = [0u8; package_length];
        loop {
            let n = c::write(socket, &msg as *const _ as *const c_void, package_length);
            if n <= 0 {
                println!("last OS error: {:?}", Error::last_os_error());
                c::close(socket);
                break;
            }
            println!("fiber-{} send {}", current_id(), String::from_utf8_lossy(&msg[0..n as usize]));
            let mut buf = [0u8; package_length];
            let n = c::read(socket, &mut buf as *mut _ as *mut c_void, package_length);
            if n <= 0 {
                println!("last OS error: {:?}", Error::last_os_error());
                break;
            }
        }

        c::close(socket);
    }
}

fn fiber_main(_: *const c_void, _: Option<*mut c_void>) {
    // create clients
    //todo 这里支持的不太好，另外需要添加统计信息
    let fiber1 = Fiber::new(fiber_request, None, 128000);
    let fiber2 = Fiber::new(fiber_request, None, 128000);
    let fiber3 = Fiber::new(fiber_request, None, 128000);
    delay(duration as u32);
    fiber1.exit();
    fiber2.exit();
    fiber3.exit();
}

fn thread_main() {
    Fiber::new(fiber_main, None, 327680);
    let scheduler = Scheduler::new(EventMode::Kernel);
    scheduler.start();
}

fn main() {
    let num_cpus = num_cpus::get();
    let pool = ThreadPool::new(num_cpus);
    for i in 0..num_cpus {
        pool.execute(|| thread_main());
    }
    pool.join();
    println!("finished !");
}