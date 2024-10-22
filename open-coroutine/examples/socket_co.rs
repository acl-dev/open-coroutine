use open_coroutine::task;
use std::io::{Error, ErrorKind, IoSlice, IoSliceMut, Read, Write};
use std::net::{Shutdown, TcpListener, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub fn start_co_server<A: ToSocketAddrs>(addr: A, server_finished: Arc<(Mutex<bool>, Condvar)>) {
    let listener = TcpListener::bind(addr).expect("start server failed");
    for stream in listener.incoming() {
        _ = task!(
            |mut socket| {
                let mut buffer1 = [0; 256];
                for _ in 0..3 {
                    assert_eq!(12, socket.read(&mut buffer1).expect("recv failed"));
                    println!("Server Received: {}", String::from_utf8_lossy(&buffer1));
                    assert_eq!(256, socket.write(&buffer1).expect("send failed"));
                    println!("Server Send");
                }
                let mut buffer2 = [0; 256];
                for _ in 0..3 {
                    let mut buffers =
                        [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
                    assert_eq!(
                        26,
                        socket.read_vectored(&mut buffers).expect("readv failed")
                    );
                    println!(
                        "Server Received Multiple: {}{}",
                        String::from_utf8_lossy(&buffer1),
                        String::from_utf8_lossy(&buffer2)
                    );
                    let responses = [IoSlice::new(&buffer1), IoSlice::new(&buffer2)];
                    assert_eq!(
                        512,
                        socket.write_vectored(&responses).expect("writev failed")
                    );
                    println!("Server Send Multiple");
                }
                println!("Server Shutdown Write");
                if socket.shutdown(Shutdown::Write).is_ok() {
                    println!("Server Closed Connection");
                    let (lock, cvar) = &*server_finished;
                    let mut pending = lock.lock().unwrap();
                    *pending = false;
                    cvar.notify_one();
                    println!("Server Closed");
                }
            },
            stream.expect("accept new connection failed"),
        );
    }
}

pub fn start_co_client<A: ToSocketAddrs>(addr: A) {
    _ = task!(
        |mut stream| {
            let mut buffer1 = [0; 256];
            for i in 0..3 {
                assert_eq!(
                    12,
                    stream
                        .write(format!("RequestPart{i}").as_ref())
                        .expect("send failed")
                );
                println!("Client Send");
                assert_eq!(256, stream.read(&mut buffer1).expect("recv failed"));
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
                assert_eq!(26, stream.write_vectored(&requests).expect("writev failed"));
                println!("Client Send Multiple");
                let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
                assert_eq!(
                    512,
                    stream.read_vectored(&mut buffers).expect("readv failed")
                );
                println!(
                    "Client Received Multiple: {}{}",
                    String::from_utf8_lossy(&buffer1),
                    String::from_utf8_lossy(&buffer2)
                );
            }
            println!("Client Shutdown Write");
            stream.shutdown(Shutdown::Write).expect("shutdown failed");
            println!("Client Closed");
        },
        open_coroutine::connect_timeout(addr, Duration::from_secs(3)).expect("connect failed"),
    );
}

#[open_coroutine::main(event_loop_size = 1, max_size = 2)]
pub fn main() -> std::io::Result<()> {
    let addr = "127.0.0.1:8999";
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    _ = std::thread::Builder::new()
        .name("crate_co_server".to_string())
        .spawn(move || start_co_server(addr, server_finished_pair))
        .expect("failed to spawn thread");
    _ = std::thread::Builder::new()
        .name("crate_co_client".to_string())
        .spawn(move || start_co_client(addr))
        .expect("failed to spawn thread");

    let (lock, cvar) = &*server_finished;
    let result = cvar
        .wait_timeout_while(
            lock.lock().unwrap(),
            Duration::from_secs(30),
            |&mut pending| pending,
        )
        .unwrap();
    if result.1.timed_out() {
        Err(Error::new(
            ErrorKind::Other,
            "The coroutine server and coroutine client did not completed within the specified time",
        ))
    } else {
        Ok(())
    }
}
