use std::os::raw::c_void;

pub use base_coroutine::*;

pub use open_coroutine_macros::*;

#[allow(dead_code)]
extern "C" {
    fn init_hook();

    fn coroutine_crate(
        f: UserFunc<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        param: Option<&'static mut c_void>,
        stack_size: usize,
    ) -> JoinHandle;

    fn coroutine_join(handle: JoinHandle) -> libc::c_long;

    fn coroutine_timeout_join(handle: &JoinHandle, ns_time: u64) -> libc::c_long;

    fn try_timed_schedule(ns_time: u64) -> libc::c_int;

    fn timed_schedule(ns_time: u64) -> libc::c_int;
}

pub fn init() {
    unsafe { init_hook() };
    println!("open-coroutine inited !");
}

pub fn co(
    f: UserFunc<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    param: Option<&'static mut c_void>,
    stack_size: usize,
) -> JoinHandle {
    unsafe { coroutine_crate(f, param, stack_size) }
}

pub fn join(handle: JoinHandle) -> libc::c_long {
    unsafe { coroutine_join(handle) }
}

pub fn timeout_join(handle: &JoinHandle, ns_time: u64) -> libc::c_long {
    unsafe { coroutine_timeout_join(handle, ns_time) }
}

pub fn schedule() -> bool {
    unsafe { try_timed_schedule(u64::MAX) == 0 }
}

#[cfg(test)]
mod tests {
    use crate::{co, init, schedule, Yielder};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::os::raw::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn test_link() {
        init();
    }

    extern "C" fn f1(
        _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        println!("[coroutine1] launched");
        input
    }

    extern "C" fn f2(
        _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        _input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        println!("[coroutine2] launched");
        None
    }

    #[test]
    fn simplest() {
        let _ = co(f1, None, 4096);
        let _ = co(f2, None, 4096);
        assert!(schedule());
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
            .as_nanos() as u64
    }

    fn hook_test(millis: u64) {
        let _ = co(f1, None, 4096);
        let _ = co(f2, None, 4096);
        let start = now();
        std::thread::sleep(Duration::from_millis(millis));
        let end = now();
        assert!(end - start >= millis);
    }

    #[test]
    fn hook_test_schedule_timeout() {
        hook_test(1)
    }

    #[test]
    fn hook_test_schedule_normal() {
        hook_test(1_000)
    }

    extern "C" fn fx(
        _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        match input {
            Some(param) => println!(
                "[coroutine] launched param:{}",
                param as *mut c_void as usize
            ),
            None => println!("[coroutine] launched"),
        }
        None
    }

    static SERVER_STARTED: AtomicBool = AtomicBool::new(false);

    unsafe fn crate_server(port: u16, server_finished: Arc<(Mutex<bool>, Condvar)>) {
        //invoke by libc::listen
        let _ = co(fx, Some(&mut *(1usize as *mut c_void)), 4096);
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let listener = TcpListener::bind("127.0.0.1:".to_owned() + &port.to_string())
            .expect(&*("bind to 127.0.0.1:".to_owned() + &port.to_string() + " failed !"));
        SERVER_STARTED.store(true, Ordering::Release);
        //invoke by libc::accept
        let _ = co(fx, Some(&mut *(2usize as *mut c_void)), 4096);
        for stream in listener.incoming() {
            let mut stream = stream.expect("accept new connection failed !");
            let mut buffer: [u8; 512] = [0; 512];
            loop {
                //invoke by libc::recv
                let _ = co(fx, Some(&mut *(6usize as *mut c_void)), 4096);
                //从流里面读内容，读到buffer中
                let bytes_read = stream.read(&mut buffer).expect("server read failed !");
                if bytes_read == 1 && buffer[0] == b'e' {
                    //如果读到的为空，说明已经结束了
                    let (lock, cvar) = &*server_finished;
                    let mut pending = lock.lock().unwrap();
                    *pending = false;
                    cvar.notify_one();
                    println!("server closed");
                    return;
                }
                assert_eq!(512, bytes_read);
                assert_eq!(data, buffer);
                //invoke by libc::send
                let _ = co(fx, Some(&mut *(7usize as *mut c_void)), 4096);
                //回写
                assert_eq!(
                    bytes_read,
                    stream
                        .write(&buffer[..bytes_read])
                        .expect("server write failed !")
                );
            }
        }
    }

    unsafe fn client_main(mut stream: TcpStream) {
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        for _ in 0..3 {
            //invoke by libc::send
            let _ = co(fx, Some(&mut *(4usize as *mut c_void)), 4096);
            //写入stream流，如果写入失败，提示“写入失败”
            assert_eq!(512, stream.write(&data).expect("Failed to write!"));

            //invoke by libc::recv
            let _ = co(fx, Some(&mut *(5usize as *mut c_void)), 4096);
            let mut reader = BufReader::new(&stream);
            //一直读到换行为止（b'\n'中的b表示字节），读到buffer里面
            assert_eq!(
                512,
                reader
                    .read_until(b'\n', &mut buffer)
                    .expect("Failed to read into buffer")
            );
            assert_eq!(&data, &buffer as &[u8]);
            buffer.clear();
        }
        //发送终止符
        assert_eq!(1, stream.write(&[b'e']).expect("Failed to write!"));
        println!("client closed");
    }

    #[test]
    fn hook_test_connect_and_poll_and_accept() -> std::io::Result<()> {
        let port = 8888;
        let clone = port.clone();
        let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
        let server_finished = Arc::clone(&server_finished_pair);
        unsafe {
            std::thread::spawn(move || crate_server(clone, server_finished_pair));
            std::thread::spawn(move || {
                //等服务端起来
                while !SERVER_STARTED.load(Ordering::Acquire) {}
                //invoke by libc::connect
                let _ = co(fx, Some(&mut *(3usize as *mut c_void)), 4096);
                let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
                let stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
                    .expect(&*("failed to 127.0.0.1:".to_owned() + &port.to_string() + " !"));
                client_main(stream)
            });

            let (lock, cvar) = &*server_finished;
            let result = cvar
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_secs(30),
                    |&mut pending| pending,
                )
                .unwrap();
            if result.1.timed_out() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "The service was not completed within the specified time",
                ))
            } else {
                Ok(())
            }
        }
    }
}
