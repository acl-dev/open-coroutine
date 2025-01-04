use std::io::{Error, ErrorKind, IoSlice, IoSliceMut, Read, Write};
use std::net::{Shutdown, TcpListener, ToSocketAddrs};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn start_server<A: ToSocketAddrs>(addr: A, server_finished: Arc<(Mutex<bool>, Condvar)>) {
    let listener = TcpListener::bind(addr).expect("start server failed");
    for stream in listener.incoming() {
        let mut socket = stream.expect("accept new connection failed");
        let mut buffer1 = [0; 256];
        for _ in 0..3 {
            assert_eq!(12, socket.read(&mut buffer1).expect("recv failed"));
            eprintln!("Server Received: {}", String::from_utf8_lossy(&buffer1));
            assert_eq!(256, socket.write(&buffer1).expect("send failed"));
            eprintln!("Server Send");
        }
        let mut buffer2 = [0; 256];
        for _ in 0..3 {
            let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
            assert_eq!(
                26,
                socket.read_vectored(&mut buffers).expect("readv failed")
            );
            eprintln!(
                "Server Received Multiple: {}{}",
                String::from_utf8_lossy(&buffer1),
                String::from_utf8_lossy(&buffer2)
            );
            let responses = [IoSlice::new(&buffer1), IoSlice::new(&buffer2)];
            assert_eq!(
                512,
                socket.write_vectored(&responses).expect("writev failed")
            );
            eprintln!("Server Send Multiple");
        }
        #[cfg(unix)]
        for _ in 0..3 {
            let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
            let mut msg = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: buffers.as_mut_ptr().cast::<libc::iovec>(),
                msg_iovlen: buffers.len() as _,
                msg_control: std::ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            };
            assert_eq!(26, unsafe {
                libc::recvmsg(socket.as_raw_fd(), &mut msg, 0)
            });
            eprintln!(
                "Server Received Message: {} {}",
                String::from_utf8_lossy(&buffer1),
                String::from_utf8_lossy(&buffer2)
            );
            assert_eq!(512, unsafe { libc::sendmsg(socket.as_raw_fd(), &msg, 0) });
            eprintln!("Server Send Message");
        }
        eprintln!("Server Shutdown Write");
        if socket.shutdown(Shutdown::Write).is_ok() {
            eprintln!("Server Closed Connection");
            let (lock, cvar) = &*server_finished;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            cvar.notify_one();
            eprintln!("Server Closed");
            return;
        }
    }
}

fn start_client<A: ToSocketAddrs>(addr: A) {
    let mut stream =
        open_coroutine::connect_timeout(addr, Duration::from_secs(1)).expect("connect failed");
    let mut buffer1 = [0; 256];
    for i in 0..3 {
        assert_eq!(
            12,
            stream
                .write(format!("RequestPart{i}").as_ref())
                .expect("send failed")
        );
        eprintln!("Client Send");
        assert_eq!(256, stream.read(&mut buffer1).expect("recv failed"));
        eprintln!("Client Received: {}", String::from_utf8_lossy(&buffer1));
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
        eprintln!("Client Send Multiple");
        let mut buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
        assert_eq!(
            512,
            stream.read_vectored(&mut buffers).expect("readv failed")
        );
        eprintln!(
            "Client Received Multiple: {}{}",
            String::from_utf8_lossy(&buffer1),
            String::from_utf8_lossy(&buffer2)
        );
    }
    #[cfg(unix)]
    for i in 0..3 {
        let mut request1 = format!("MessagePart{i}1").into_bytes();
        let mut request2 = format!("MessagePart{i}2").into_bytes();
        let mut buffers = [
            IoSliceMut::new(request1.as_mut_slice()),
            IoSliceMut::new(request2.as_mut_slice()),
        ];
        let mut msg = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: buffers.as_mut_ptr().cast::<libc::iovec>(),
            msg_iovlen: buffers.len() as _,
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };
        assert_eq!(26, unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) });
        eprintln!("Client Send Message");
        buffers = [IoSliceMut::new(&mut buffer1), IoSliceMut::new(&mut buffer2)];
        msg.msg_iov = buffers.as_mut_ptr().cast::<libc::iovec>();
        msg.msg_iovlen = buffers.len() as _;
        assert_eq!(512, unsafe {
            libc::recvmsg(stream.as_raw_fd(), &mut msg, 0)
        });
        eprintln!(
            "Client Received Message: {}{}",
            String::from_utf8_lossy(&buffer1),
            String::from_utf8_lossy(&buffer2)
        );
    }
    eprintln!("Client Shutdown Write");
    stream.shutdown(Shutdown::Write).expect("shutdown failed");
    eprintln!("Client Closed");
}

#[open_coroutine::main(event_loop_size = 1, max_size = 1)]
pub fn main() -> std::io::Result<()> {
    let addr = "127.0.0.1:8888";
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    _ = std::thread::Builder::new()
        .name("crate_server".to_string())
        .spawn(move || start_server(addr, server_finished_pair))
        .expect("failed to spawn thread");
    _ = std::thread::Builder::new()
        .name("crate_client".to_string())
        .spawn(move || start_client(addr))
        .expect("failed to spawn thread");

    let (lock, cvar) = &*server_finished;
    let result = cvar
        .wait_timeout_while(
            lock.lock().unwrap(),
            Duration::from_secs(10),
            |&mut pending| pending,
        )
        .unwrap();
    if result.1.timed_out() {
        Err(Error::new(
            ErrorKind::Other,
            "The service did not completed within the specified time",
        ))
    } else {
        Ok(())
    }
}
