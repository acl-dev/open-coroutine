use open_coroutine::task;
use std::io::{BufRead, BufReader, ErrorKind, IoSlice, IoSliceMut, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn crate_task(input: i32) {
    _ = task!(
        |_, param| {
            println!("[coroutine{}] launched", param);
        },
        input,
    );
}

pub fn start_server<A: ToSocketAddrs>(
    addr: A,
    server_finished: Arc<(Mutex<bool>, Condvar)>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut buffer1 = [0; 256];
        for _ in 0..3 {
            assert_eq!(12, stream.read(&mut buffer1)?);
            println!("Server Received: {}", String::from_utf8_lossy(&buffer1));
            assert_eq!(256, stream.write(&buffer1)?);
            println!("Server Send");
        }
        let mut buffer2 = [0; 256];
        for _ in 0..3 {
            let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
            assert_eq!(26, stream.read_vectored(&mut buffers)?);
            println!(
                "Server Received Multiple: {}{}",
                String::from_utf8_lossy(&buffer1),
                String::from_utf8_lossy(&buffer2)
            );
            let responses = [IoSlice::new(&buffer1), IoSlice::new(&buffer2)];
            assert_eq!(512, stream.write_vectored(&responses)?);
            println!("Server Send Multiple");
        }
        println!("Server Shutdown Write");
        stream.shutdown(Shutdown::Write).map(|()| {
            println!("Server Closed Connection");
        })?;
        let (lock, cvar) = &*server_finished;
        let mut pending = lock.lock().unwrap();
        *pending = false;
        cvar.notify_one();
    }
    Ok(())
}

pub fn start_client<A: ToSocketAddrs>(addr: A) -> std::io::Result<()> {
    let mut stream = connect_timeout(addr, Duration::from_secs(3))?;
    let mut buffer1 = [0; 256];
    for i in 0..3 {
        assert_eq!(12, stream.write(format!("RequestPart{i}").as_ref())?);
        println!("Client Send");
        assert_eq!(256, stream.read(&mut buffer1)?);
        println!("Client Received: {}", String::from_utf8_lossy(&buffer1));
    }
    let mut buffer2 = [0; 256];
    for i in 0..3 {
        let request1 = format!("RequestPart{i}1");
        let request2 = format!("RequestPart{i}2");
        let requests = [
            IoSlice::new(request1.as_ref()),
            IoSlice::new(request2.as_ref()),
        ];
        assert_eq!(26, stream.write_vectored(&requests)?);
        println!("Client Send Multiple");
        let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
        assert_eq!(512, stream.read_vectored(&mut buffers)?);
        println!(
            "Client Received Multiple: {}{}",
            String::from_utf8_lossy(&buffer1),
            String::from_utf8_lossy(&buffer2)
        );
    }
    println!("Client Shutdown Write");
    stream.shutdown(Shutdown::Write).map(|()| {
        println!("Client Closed");
    })
}

fn connect_timeout<A: ToSocketAddrs>(addr: A, timeout: Duration) -> std::io::Result<TcpStream> {
    let mut last_err = None;
    for addr in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(l) => return Ok(l),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            "could not resolve to any addresses",
        )
    }))
}

pub fn crate_server(
    port: u16,
    server_started: Arc<AtomicBool>,
    server_finished: Arc<(Mutex<bool>, Condvar)>,
) {
    //invoke by libc::listen
    crate_task(1);
    let mut data: [u8; 512] = unsafe { std::mem::zeroed() };
    data[511] = b'\n';
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .unwrap_or_else(|_| panic!("bind to 127.0.0.1:{port} failed !"));
    server_started.store(true, Ordering::Release);
    //invoke by libc::accept
    crate_task(2);
    if let Some(stream) = listener.incoming().next() {
        let mut stream = stream.expect("accept new connection failed !");
        let mut buffer: [u8; 512] = [0; 512];
        loop {
            //invoke by libc::recv
            crate_task(6);
            //从流里面读内容，读到buffer中
            let bytes_read = stream.read(&mut buffer).expect("server read failed !");
            if bytes_read == 0 {
                println!("server close a connection");
                continue;
            }
            print!("Server Received: {}", String::from_utf8_lossy(&buffer[..]));
            if bytes_read == 1 && buffer[0] == b'e' {
                //如果读到的为空，说明已经结束了
                let (lock, cvar) = &*server_finished;
                let mut pending = lock.lock().unwrap();
                *pending = false;
                cvar.notify_one();
                println!("server closed");
                crate_task(8);
                return;
            }
            assert_eq!(512, bytes_read);
            assert_eq!(data, buffer);
            //invoke by libc::send
            crate_task(7);
            //回写
            assert_eq!(
                bytes_read,
                stream
                    .write(&buffer[..bytes_read])
                    .expect("server write failed !")
            );
            print!(
                "Server Send: {}",
                String::from_utf8_lossy(&buffer[..bytes_read])
            );
        }
    }
}

pub fn crate_client(port: u16, server_started: Arc<AtomicBool>) {
    //等服务端起来
    while !server_started.load(Ordering::Acquire) {}
    //invoke by libc::connect
    crate_task(3);
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
    let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
        .unwrap_or_else(|_| panic!("connect to 127.0.0.1:{port} failed !"));
    let mut data: [u8; 512] = unsafe { std::mem::zeroed() };
    data[511] = b'\n';
    let mut buffer: Vec<u8> = Vec::with_capacity(512);
    for _ in 0..3 {
        //invoke by libc::send
        crate_task(4);
        //写入stream流，如果写入失败，提示"写入失败"
        assert_eq!(512, stream.write(&data).expect("Failed to write!"));
        print!("Client Send: {}", String::from_utf8_lossy(&data[..]));

        //invoke by libc::recv
        crate_task(5);
        let mut reader = BufReader::new(&stream);
        //一直读到换行为止（b'\n'中的b表示字节），读到buffer里面
        assert_eq!(
            512,
            reader
                .read_until(b'\n', &mut buffer)
                .expect("Failed to read into buffer")
        );
        print!("Client Received: {}", String::from_utf8_lossy(&buffer[..]));
        assert_eq!(&data, &buffer as &[u8]);
        buffer.clear();
    }
    //发送终止符
    assert_eq!(1, stream.write(&[b'e']).expect("Failed to write!"));
    println!("client closed");
    crate_task(8);
}

pub fn crate_co_server(
    port: u16,
    server_started: Arc<AtomicBool>,
    server_finished: Arc<(Mutex<bool>, Condvar)>,
) {
    //invoke by libc::listen
    crate_task(11);
    let mut data: [u8; 512] = unsafe { std::mem::zeroed() };
    data[511] = b'\n';
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .unwrap_or_else(|_| panic!("bind to 127.0.0.1:{port} failed !"));
    server_started.store(true, Ordering::Release);
    //invoke by libc::accept
    crate_task(12);
    for stream in listener.incoming() {
        _ = task!(
            |_, input| {
                let mut stream = input.expect("accept new connection failed !");
                let mut buffer: [u8; 512] = [0; 512];
                loop {
                    //invoke by libc::recv
                    crate_task(16);
                    //从流里面读内容，读到buffer中
                    let bytes_read = stream
                        .read(&mut buffer)
                        .expect("coroutine server read failed !");
                    if bytes_read == 0 {
                        println!("coroutine server close a connection");
                        return None;
                    }
                    print!(
                        "Coroutine Server Received: {}",
                        String::from_utf8_lossy(&buffer[..])
                    );
                    if bytes_read == 1 && buffer[0] == b'e' {
                        //如果读到的为空，说明已经结束了
                        let (lock, cvar) = &*server_finished;
                        let mut pending = lock.lock().unwrap();
                        *pending = false;
                        cvar.notify_one();
                        println!("coroutine server closed");
                        crate_task(18);
                        return Some(Box::leak(Box::new(stream)));
                    }
                    assert_eq!(512, bytes_read);
                    assert_eq!(data, buffer);
                    //invoke by libc::send
                    crate_task(17);
                    //回写
                    assert_eq!(
                        bytes_read,
                        stream
                            .write(&buffer[..bytes_read])
                            .expect("coroutine server write failed !")
                    );
                    print!(
                        "Coroutine Server Send: {}",
                        String::from_utf8_lossy(&buffer[..bytes_read])
                    );
                }
            },
            stream,
        );
    }
}

pub fn crate_co_client(port: u16, server_started: Arc<AtomicBool>) {
    //等服务端起来
    while !server_started.load(Ordering::Acquire) {}
    _ = task!(
        |_, input| {
            //invoke by libc::connect
            crate_task(13);
            let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), input);
            let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
                .unwrap_or_else(|_| panic!("connect to 127.0.0.1:{input} failed !"));
            let mut data: [u8; 512] = unsafe { std::mem::zeroed() };
            data[511] = b'\n';
            let mut buffer: Vec<u8> = Vec::with_capacity(512);
            for _ in 0..3 {
                //invoke by libc::send
                crate_task(14);
                //写入stream流，如果写入失败，提示"写入失败"
                assert_eq!(512, stream.write(&data).expect("Failed to write!"));
                print!(
                    "Coroutine Client Send: {}",
                    String::from_utf8_lossy(&data[..])
                );

                //invoke by libc::recv
                crate_task(15);
                let mut reader = BufReader::new(&stream);
                //一直读到换行为止（b'\n'中的b表示字节），读到buffer里面
                assert_eq!(
                    512,
                    reader
                        .read_until(b'\n', &mut buffer)
                        .expect("Failed to read into buffer")
                );
                print!(
                    "Coroutine Client Received: {}",
                    String::from_utf8_lossy(&buffer[..])
                );
                assert_eq!(&data, &buffer as &[u8]);
                buffer.clear();
            }
            //发送终止符
            assert_eq!(1, stream.write(&[b'e']).expect("Failed to write!"));
            println!("coroutine client closed");
            crate_task(18);
        },
        port,
    );
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
        .as_nanos() as u64
}

pub fn sleep_test(millis: u64) {
    _ = task!(
        move |_, _| {
            println!("[coroutine1] {millis} launched");
        },
        (),
    );
    _ = task!(
        move |_, _| {
            println!("[coroutine2] {millis} launched");
        },
        (),
    );
    let start = now();
    std::thread::sleep(Duration::from_millis(millis));
    let end = now();
    assert!(end - start >= millis, "Time consumption less than expected");
}

pub fn sleep_test_co(millis: u64) {
    _ = task!(
        move |_, _| {
            let start = now();
            std::thread::sleep(Duration::from_millis(millis));
            let end = now();
            assert!(end - start >= millis, "Time consumption less than expected");
            println!("[coroutine1] {millis} launched");
        },
        (),
    );
    _ = task!(
        move |_, _| {
            std::thread::sleep(Duration::from_millis(500));
            println!("[coroutine2] {millis} launched");
        },
        (),
    );
    std::thread::sleep(Duration::from_millis(millis + 500));
}
