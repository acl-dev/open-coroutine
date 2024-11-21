use crate::net::operator::Operator;
use slab::Slab;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::windows::io::AsRawSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows_sys::Win32::Networking::WinSock::{closesocket, SOCKET};

#[derive(Clone, Debug)]
enum Token {
    Accept,
    Read {
        fd: SOCKET,
        buf_index: usize,
    },
    Write {
        fd: SOCKET,
        buf_index: usize,
        offset: usize,
        len: usize,
    },
}

fn crate_client(port: u16, server_started: Arc<AtomicBool>) {
    //等服务端起来
    while !server_started.load(Ordering::Acquire) {}
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
    let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
        .unwrap_or_else(|_| panic!("connect to 127.0.0.1:{port} failed !"));
    let mut data: [u8; 512] = [b'1'; 512];
    data[511] = b'\n';
    let mut buffer: Vec<u8> = Vec::with_capacity(512);
    for _ in 0..3 {
        //写入stream流，如果写入失败，提示"写入失败"
        assert_eq!(512, stream.write(&data).expect("Failed to write!"));
        print!("Client Send: {}", String::from_utf8_lossy(&data[..]));

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
}

fn crate_server2(port: u16, server_started: Arc<AtomicBool>) -> anyhow::Result<()> {
    let operator = Operator::new(0)?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;

    let mut bufpool = Vec::with_capacity(64);
    let mut buf_alloc = Slab::with_capacity(64);
    let mut token_alloc = Slab::with_capacity(64);

    println!("listen {}", listener.local_addr()?);
    server_started.store(true, Ordering::Release);

    operator.accept(
        token_alloc.insert(Token::Accept),
        listener.as_raw_socket() as _,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    )?;

    loop {
        let (_, mut cq, _) = operator.select(None, 1)?;
        for cqe in &mut cq {
            let token_index = cqe.token;
            let token = &mut token_alloc[token_index];
            match token.clone() {
                Token::Accept => {
                    println!("accept");
                    let fd = cqe.socket;
                    let (buf_index, buf) = match bufpool.pop() {
                        Some(buf_index) => (buf_index, &mut buf_alloc[buf_index]),
                        None => {
                            let buf = vec![0u8; 2048].into_boxed_slice();
                            let buf_entry = buf_alloc.vacant_entry();
                            let buf_index = buf_entry.key();
                            (buf_index, buf_entry.insert(buf))
                        }
                    };
                    *token = Token::Read { fd, buf_index };
                    operator.recv(token_index, fd, buf.as_mut_ptr() as _, buf.len() as _, 0)?;
                }
                Token::Read { fd, buf_index } => {
                    let ret = cqe.bytes_transferred as _;
                    if ret == 0 {
                        bufpool.push(buf_index);
                        _ = token_alloc.remove(token_index);
                        println!("shutdown connection");
                        _ = unsafe { closesocket(fd) };
                        println!("Server closed");
                        return Ok(());
                    } else {
                        let len = ret;
                        let buf = &buf_alloc[buf_index];
                        *token = Token::Write {
                            fd,
                            buf_index,
                            len,
                            offset: 0,
                        };
                        operator.send(token_index, fd, buf.as_ptr() as _, len as _, 0)?;
                    }
                }
                Token::Write {
                    fd,
                    buf_index,
                    offset,
                    len,
                } => {
                    let write_len = cqe.bytes_transferred as usize;
                    if offset + write_len >= len {
                        bufpool.push(buf_index);
                        let (buf_index, buf) = match bufpool.pop() {
                            Some(buf_index) => (buf_index, &mut buf_alloc[buf_index]),
                            None => {
                                let buf = vec![0u8; 2048].into_boxed_slice();
                                let buf_entry = buf_alloc.vacant_entry();
                                let buf_index = buf_entry.key();
                                (buf_index, buf_entry.insert(buf))
                            }
                        };
                        *token = Token::Read { fd, buf_index };
                        operator.recv(token_index, fd, buf.as_mut_ptr() as _, buf.len() as _, 0)?;
                    } else {
                        let offset = offset + write_len;
                        let len = len - offset;
                        let buf = &buf_alloc[buf_index][offset..];
                        *token = Token::Write {
                            fd,
                            buf_index,
                            offset,
                            len,
                        };
                        operator.write(token_index, fd as _, buf.as_ptr() as _, len as _)?;
                    };
                }
            }
        }
    }
}

#[test]
fn framework() -> anyhow::Result<()> {
    #[cfg(feature = "log")]
    let _ = tracing_subscriber::fmt()
        .with_thread_names(true)
        .with_line_number(true)
        .with_timer(tracing_subscriber::fmt::time::OffsetTime::new(
            time::UtcOffset::from_hms(8, 0, 0).expect("create UtcOffset failed !"),
            time::format_description::well_known::Rfc2822,
        ))
        .try_init();
    let port = 7061;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let handle = std::thread::spawn(move || crate_server2(port, clone));
    std::thread::spawn(move || crate_client(port, server_started))
        .join()
        .expect("client has error");
    handle.join().expect("server has error")
}
