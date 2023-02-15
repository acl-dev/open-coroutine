use std::os::raw::c_void;

pub use open_coroutine_core::*;

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

pub fn co<F, P, R: 'static>(f: F, param: Option<&'static mut P>, stack_size: usize) -> JoinHandle
where
    F: FnOnce(
            &'static Yielder<Option<&'static mut P>, (), Option<&'static mut R>>,
            Option<&'static mut P>,
        ) -> Option<&'static mut R>
        + Copy,
{
    extern "C" fn co_main<F, P: 'static, R: 'static>(
        yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void>
    where
        F: FnOnce(
                &'static Yielder<Option<&'static mut P>, (), Option<&'static mut R>>,
                Option<&'static mut P>,
            ) -> Option<&'static mut R>
            + Copy,
    {
        unsafe {
            let ptr: &mut (F, Option<&'static mut P>) = std::mem::transmute(input.unwrap());
            let data = std::ptr::read_unaligned(ptr);
            let result = (data.0)(std::mem::transmute(yielder), data.1);
            result.map(|p| std::mem::transmute(p))
        }
    }
    let inner = Box::leak(Box::new((f, param)));
    unsafe {
        coroutine_crate(
            co_main::<F, P, R>,
            Some(std::mem::transmute(inner)),
            stack_size,
        )
    }
}

#[macro_export]
macro_rules! co {
    ( $f: expr , $param:expr $(,)? ) => {{
        $crate::co($f, $param, open_coroutine_core::Stack::default_size())
    }};
    ( $f: expr , $param:expr ,$stack_size: expr $(,)?) => {{
        $crate::co($f, $param, $stack_size)
    }};
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
    use crate::{co, coroutine_crate, init, schedule, JoinHandle, UserFunc, Yielder};
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

    #[test]
    fn simplest() {
        let _ = co!(
            |_yielder, input: Option<&'static mut c_void>| {
                println!("[coroutine1] launched");
                input
            },
            None,
            4096,
        );
        let _ = co!(
            |_yielder, input: Option<&'static mut c_void>| {
                println!("[coroutine2] launched");
                input
            },
            None,
        );
        assert!(schedule());
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
            .as_nanos() as u64
    }

    fn hook_test(millis: u64) {
        let _ = co(
            |_yielder, input: Option<&'static mut c_void>| {
                println!("[coroutine1] launched");
                input
            },
            None,
            4096,
        );
        let _ = co(
            |_yielder, input: Option<&'static mut c_void>| {
                println!("[coroutine2] launched");
                input
            },
            None,
            4096,
        );
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

    fn co_crate(
        f: UserFunc<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        param: Option<&'static mut c_void>,
        stack_size: usize,
    ) -> JoinHandle {
        unsafe { coroutine_crate(f, param, stack_size) }
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

    unsafe fn crate_server(
        port: u16,
        server_started: Arc<AtomicBool>,
        server_finished: Arc<(Mutex<bool>, Condvar)>,
    ) {
        //invoke by libc::listen
        let _ = co_crate(fx, Some(&mut *(1usize as *mut c_void)), 4096);
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .expect(&format!("bind to 127.0.0.1:{port} failed !"));
        server_started.store(true, Ordering::Release);
        //invoke by libc::accept
        let _ = co_crate(fx, Some(&mut *(2usize as *mut c_void)), 4096);
        for stream in listener.incoming() {
            let mut stream = stream.expect("accept new connection failed !");
            let mut buffer: [u8; 512] = [0; 512];
            loop {
                //invoke by libc::recv
                let _ = co_crate(fx, Some(&mut *(6usize as *mut c_void)), 4096);
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
                let _ = co_crate(fx, Some(&mut *(7usize as *mut c_void)), 4096);
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

    unsafe fn client_main(port: u16, server_started: Arc<AtomicBool>) {
        //等服务端起来
        while !server_started.load(Ordering::Acquire) {}
        //invoke by libc::connect
        let _ = co_crate(fx, Some(&mut *(3usize as *mut c_void)), 4096);
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
        let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
            .expect(&format!("connect to 127.0.0.1:{port} failed !"));
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        for _ in 0..3 {
            //invoke by libc::send
            let _ = co_crate(fx, Some(&mut *(4usize as *mut c_void)), 4096);
            //写入stream流，如果写入失败，提示“写入失败”
            assert_eq!(512, stream.write(&data).expect("Failed to write!"));

            //invoke by libc::recv
            let _ = co_crate(fx, Some(&mut *(5usize as *mut c_void)), 4096);
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
        let server_started = Arc::new(AtomicBool::new(false));
        let clone = server_started.clone();
        let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
        let server_finished = Arc::clone(&server_finished_pair);
        unsafe {
            std::thread::spawn(move || crate_server(port, clone, server_finished_pair));
            std::thread::spawn(move || client_main(port, server_started));

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

    unsafe fn crate_co_server(
        port: u16,
        server_started: Arc<AtomicBool>,
        server_finished: Arc<(Mutex<bool>, Condvar)>,
    ) {
        //invoke by libc::listen
        let _ = co_crate(fx, Some(&mut *(11usize as *mut c_void)), 4096);
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .expect(&format!("bind to 127.0.0.1:{port} failed !"));
        server_started.store(true, Ordering::Release);
        //invoke by libc::accept
        let _ = co_crate(fx, Some(&mut *(12usize as *mut c_void)), 4096);
        for stream in listener.incoming() {
            let leaked = Box::leak(Box::new(stream));
            let _ = co(
                |_yielder, input: Option<&'static mut std::io::Result<TcpStream>>| {
                    let mut stream = std::ptr::read_unaligned(input.unwrap())
                        .expect("accept new connection failed !");
                    let mut buffer: [u8; 512] = [0; 512];
                    loop {
                        //invoke by libc::recv
                        let _ = co_crate(fx, Some(&mut *(16usize as *mut c_void)), 4096);
                        //从流里面读内容，读到buffer中
                        let bytes_read = stream
                            .read(&mut buffer)
                            .expect("coroutine server read failed !");
                        if bytes_read == 1 && buffer[0] == b'e' {
                            //如果读到的为空，说明已经结束了
                            let (lock, cvar) = &*server_finished;
                            let mut pending = lock.lock().unwrap();
                            *pending = false;
                            cvar.notify_one();
                            println!("coroutine server closed");
                            return Some(Box::leak(Box::new(stream)));
                        }
                        assert_eq!(512, bytes_read);
                        assert_eq!(data, buffer);
                        //invoke by libc::send
                        let _ = co_crate(fx, Some(&mut *(17usize as *mut c_void)), 4096);
                        //回写
                        assert_eq!(
                            bytes_read,
                            stream
                                .write(&buffer[..bytes_read])
                                .expect("coroutine server write failed !")
                        );
                    }
                },
                Some(leaked),
                4096,
            )
            .join();
        }
    }

    #[test]
    fn hook_test_co_server() -> std::io::Result<()> {
        let port = 8889;
        let server_started = Arc::new(AtomicBool::new(false));
        let clone = server_started.clone();
        let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
        let server_finished = Arc::clone(&server_finished_pair);
        unsafe {
            std::thread::spawn(move || crate_co_server(port, clone, server_finished_pair));
            std::thread::spawn(move || client_main(port, server_started));

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
                    "The coroutine service was not completed within the specified time",
                ))
            } else {
                Ok(())
            }
        }
    }
}
