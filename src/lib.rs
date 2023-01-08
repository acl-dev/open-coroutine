use std::os::raw::c_void;

pub use base_coroutine::*;

#[allow(dead_code)]
extern "C" {
    fn init_hook();

    fn coroutine_crate(
        f: UserFunc<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        param: Option<&'static mut c_void>,
        stack_size: usize,
    ) -> libc::c_int;

    fn try_timed_schedule(ns_time: u64) -> libc::c_int;

    fn timed_schedule(ns_time: u64) -> libc::c_int;
}

pub fn init() {
    unsafe { init_hook() }
}

pub fn co(
    f: UserFunc<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    param: Option<&'static mut c_void>,
    stack_size: usize,
) -> bool {
    unsafe { coroutine_crate(f, param, stack_size) == 0 }
}

pub fn schedule() -> bool {
    unsafe { try_timed_schedule(u64::MAX) == 0 }
}

#[cfg(test)]
mod tests {
    use crate::{co, init, schedule, Yielder};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::os::raw::c_void;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn test_link() {
        init();
    }

    extern "C" fn f1(
        _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
        _input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        println!("[coroutine1] launched");
        None
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
        assert!(co(f1, None, 4096));
        assert!(co(f2, None, 4096));
        assert!(schedule());
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
            .as_nanos() as u64
    }

    fn hook_test(millis: u64) {
        assert!(co(f1, None, 4096));
        assert!(co(f2, None, 4096));
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

    static mut SERVER_STARTED: bool = false;

    unsafe fn crate_server() {
        //invoke by libc::listen
        assert!(co(fx, Some(&mut *(1usize as *mut c_void)), 4096));
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let listener =
            TcpListener::bind("127.0.0.1:9999").expect("bind to 127.0.0.1:9999 failed !");
        SERVER_STARTED = true;
        //invoke by libc::accept
        assert!(co(fx, Some(&mut *(2usize as *mut c_void)), 4096));
        for stream in listener.incoming() {
            let mut stream = stream.expect("accept new connection failed !");
            let mut buffer: [u8; 512] = [0; 512];
            loop {
                //invoke by libc::recv
                assert!(co(fx, Some(&mut *(6usize as *mut c_void)), 4096));
                //从流里面读内容，读到buffer中
                let bytes_read = stream.read(&mut buffer).expect("server read failed !");
                if bytes_read == 0 {
                    //如果读到的为空，说明已经结束了
                    return;
                }
                assert_eq!(data, buffer);
                //invoke by libc::send
                assert!(co(fx, Some(&mut *(7usize as *mut c_void)), 4096));
                //回写
                stream
                    .write(&buffer[..bytes_read])
                    .expect("server write failed !");
            }
        }
    }

    unsafe fn crate_client() {
        //等服务端起来
        while !SERVER_STARTED {}
        let mut data: [u8; 512] = std::mem::zeroed();
        data[511] = b'\n';
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        //invoke by libc::connect
        assert!(co(fx, Some(&mut *(3usize as *mut c_void)), 4096));
        let mut stream = TcpStream::connect("127.0.0.1:9999").expect("failed to 127.0.0.1:9999 !");
        for _ in 0..3 {
            //invoke by libc::send
            assert!(co(fx, Some(&mut *(4usize as *mut c_void)), 4096));
            //写入stream流，如果写入失败，提示“写入失败”
            stream.write(&data).expect("Failed to write!");

            //invoke by libc::recv
            assert!(co(fx, Some(&mut *(5usize as *mut c_void)), 4096));
            let mut reader = BufReader::new(&stream);
            //一直读到换行为止（b'\n'中的b表示字节），读到buffer里面
            reader
                .read_until(b'\n', &mut buffer)
                .expect("Failed to read into buffer");
            assert_eq!(&data, &buffer as &[u8]);
            buffer.clear();
        }
        //发送终止符
        stream.write(&[]).expect("Failed to write!");
    }

    #[test]
    fn hook_test_accept_and_connect() {
        unsafe {
            let handle = std::thread::spawn(|| crate_server());
            crate_client();
            //fixme 这里有个系统调用被Monitor发送的signal打断了，不知道是哪个系统调用
            let _ = handle.join();
        }
    }
}
