use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn crate_co(input: i32) {
    _ = co!(
        |_, param| {
            println!("[coroutine] launched param:{}", param);
        },
        input,
        4096,
    );
}

unsafe fn crate_server(
    port: u16,
    server_started: Arc<AtomicBool>,
    server_finished: Arc<(Mutex<bool>, Condvar)>,
) {
    //invoke by libc::listen
    crate_co(1);
    let mut data: [u8; 512] = std::mem::zeroed();
    data[511] = b'\n';
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .expect(&format!("bind to 127.0.0.1:{port} failed !"));
    server_started.store(true, Ordering::Release);
    //invoke by libc::accept
    crate_co(2);
    for stream in listener.incoming() {
        let mut stream = stream.expect("accept new connection failed !");
        let mut buffer: [u8; 512] = [0; 512];
        loop {
            //invoke by libc::recv
            crate_co(6);
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
            crate_co(7);
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

unsafe fn crate_client(port: u16, server_started: Arc<AtomicBool>) {
    //等服务端起来
    while !server_started.load(Ordering::Acquire) {}
    //invoke by libc::connect
    crate_co(3);
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
    let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
        .expect(&format!("connect to 127.0.0.1:{port} failed !"));
    let mut data: [u8; 512] = std::mem::zeroed();
    data[511] = b'\n';
    let mut buffer: Vec<u8> = Vec::with_capacity(512);
    for _ in 0..3 {
        //invoke by libc::send
        crate_co(4);
        //写入stream流，如果写入失败，提示“写入失败”
        assert_eq!(512, stream.write(&data).expect("Failed to write!"));

        //invoke by libc::recv
        crate_co(5);
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
fn hook_test_not_co() -> std::io::Result<()> {
    let port = 8888;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    unsafe {
        _ = std::thread::spawn(move || crate_server(port, clone, server_finished_pair));
        _ = std::thread::spawn(move || crate_client(port, server_started));

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
                "The service did not completed within the specified time",
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
    crate_co(11);
    let mut data: [u8; 512] = std::mem::zeroed();
    data[511] = b'\n';
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .expect(&format!("bind to 127.0.0.1:{port} failed !"));
    server_started.store(true, Ordering::Release);
    //invoke by libc::accept
    crate_co(12);
    for stream in listener.incoming() {
        _ = co!(
            |_, input| {
                let mut stream = input.expect("accept new connection failed !");
                let mut buffer: [u8; 512] = [0; 512];
                loop {
                    //invoke by libc::recv
                    crate_co(16);
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
                    crate_co(17);
                    //回写
                    assert_eq!(
                        bytes_read,
                        stream
                            .write(&buffer[..bytes_read])
                            .expect("coroutine server write failed !")
                    );
                }
            },
            stream,
        );
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
        _ = std::thread::spawn(move || crate_co_server(port, clone, server_finished_pair));
        _ = std::thread::spawn(move || crate_client(port, server_started));

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
                "The coroutine service did not completed within the specified time",
            ))
        } else {
            Ok(())
        }
    }
}

unsafe fn crate_co_client(port: u16, server_started: Arc<AtomicBool>) {
    //等服务端起来
    while !server_started.load(Ordering::Acquire) {}
    _ = co!(
        |_, input| {
            //invoke by libc::connect
            crate_co(13);
            let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), input);
            let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
                .expect(&format!("connect to 127.0.0.1:{input} failed !"));
            let mut data: [u8; 512] = std::mem::zeroed();
            data[511] = b'\n';
            let mut buffer: Vec<u8> = Vec::with_capacity(512);
            for _ in 0..3 {
                //invoke by libc::send
                crate_co(14);
                //写入stream流，如果写入失败，提示“写入失败”
                assert_eq!(512, stream.write(&data).expect("Failed to write!"));

                //invoke by libc::recv
                crate_co(15);
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
            println!("coroutine client closed");
        },
        port,
    );
}

#[test]
fn hook_test_co_client() -> std::io::Result<()> {
    let port = 8899;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    unsafe {
        _ = std::thread::spawn(move || crate_server(port, clone, server_finished_pair));
        _ = std::thread::spawn(move || crate_co_client(port, server_started));

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
                "The coroutine client did not completed within the specified time",
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn hook_test_co() -> std::io::Result<()> {
    let port = 8999;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    unsafe {
        _ = std::thread::spawn(move || crate_co_server(port, clone, server_finished_pair));
        _ = std::thread::spawn(move || crate_co_client(port, server_started));

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
                "The coroutine server and coroutine client did not completed within the specified time",
            ))
        } else {
            Ok(())
        }
    }
}
