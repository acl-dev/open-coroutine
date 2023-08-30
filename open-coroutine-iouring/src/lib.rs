#[cfg(target_os = "linux")]
pub mod version;

#[cfg(target_os = "linux")]
pub mod io_uring;

#[cfg(all(target_os = "linux", test))]
mod tests {
    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use std::{io, ptr};

    use crate::io_uring::IoUringOperator;
    use io_uring::{opcode, squeue, types, IoUring, SubmissionQueue};
    use slab::Slab;

    #[derive(Clone, Debug)]
    enum Token {
        Accept,
        Poll {
            fd: RawFd,
        },
        Read {
            fd: RawFd,
            buf_index: usize,
        },
        Write {
            fd: RawFd,
            buf_index: usize,
            offset: usize,
            len: usize,
        },
    }

    pub struct AcceptCount {
        entry: squeue::Entry,
        count: usize,
    }

    impl AcceptCount {
        fn new(fd: RawFd, token: usize, count: usize) -> AcceptCount {
            AcceptCount {
                entry: opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                    .build()
                    .user_data(token as _),
                count,
            }
        }

        pub fn push_to(&mut self, sq: &mut SubmissionQueue<'_>) {
            while self.count > 0 {
                unsafe {
                    match sq.push(&self.entry) {
                        Ok(_) => self.count -= 1,
                        Err(_) => break,
                    }
                }
            }

            sq.sync();
        }
    }

    pub fn crate_server(port: u16, server_started: Arc<AtomicBool>) -> anyhow::Result<()> {
        let mut ring: IoUring = IoUring::builder()
            .setup_sqpoll(1000)
            .setup_sqpoll_cpu(0)
            .build(1024)?;
        let listener = TcpListener::bind(("127.0.0.1", port))?;

        let mut backlog = VecDeque::new();
        let mut bufpool = Vec::with_capacity(64);
        let mut buf_alloc = Slab::with_capacity(64);
        let mut token_alloc = Slab::with_capacity(64);

        println!("listen {}", listener.local_addr()?);
        server_started.store(true, Ordering::Release);

        let (submitter, mut sq, mut cq) = ring.split();

        let mut accept =
            AcceptCount::new(listener.as_raw_fd(), token_alloc.insert(Token::Accept), 1);

        accept.push_to(&mut sq);

        loop {
            match submitter.submit_and_wait(1) {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
                Err(err) => return Err(err.into()),
            }
            cq.sync();

            // clean backlog
            loop {
                if sq.is_full() {
                    match submitter.submit() {
                        Ok(_) => (),
                        Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                        Err(err) => return Err(err.into()),
                    }
                }
                sq.sync();

                match backlog.pop_front() {
                    Some(sqe) => unsafe {
                        let _ = sq.push(&sqe);
                    },
                    None => break,
                }
            }

            accept.push_to(&mut sq);

            for cqe in &mut cq {
                let ret = cqe.result();
                let token_index = cqe.user_data() as usize;

                if ret < 0 {
                    eprintln!(
                        "token {:?} error: {:?}",
                        token_alloc.get(token_index),
                        io::Error::from_raw_os_error(-ret)
                    );
                    continue;
                }

                let token = &mut token_alloc[token_index];
                match token.clone() {
                    Token::Accept => {
                        println!("accept");

                        accept.count += 1;

                        let fd = ret;
                        let poll_token = token_alloc.insert(Token::Poll { fd });

                        let poll_e = opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                            .build()
                            .user_data(poll_token as _);

                        unsafe {
                            if sq.push(&poll_e).is_err() {
                                backlog.push_back(poll_e);
                            }
                        }
                    }
                    Token::Poll { fd } => {
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

                        let read_e =
                            opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as _)
                                .build()
                                .user_data(token_index as _);

                        unsafe {
                            if sq.push(&read_e).is_err() {
                                backlog.push_back(read_e);
                            }
                        }
                    }
                    Token::Read { fd, buf_index } => {
                        if ret == 0 {
                            bufpool.push(buf_index);
                            token_alloc.remove(token_index);
                            println!("shutdown connection");
                            unsafe { libc::close(fd) };

                            println!("Server closed");
                            return Ok(());
                        } else {
                            let len = ret as usize;
                            let buf = &buf_alloc[buf_index];

                            *token = Token::Write {
                                fd,
                                buf_index,
                                len,
                                offset: 0,
                            };

                            let write_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), len as _)
                                .build()
                                .user_data(token_index as _);

                            unsafe {
                                if sq.push(&write_e).is_err() {
                                    backlog.push_back(write_e);
                                }
                            }
                        }
                    }
                    Token::Write {
                        fd,
                        buf_index,
                        offset,
                        len,
                    } => {
                        let write_len = ret as usize;

                        let entry = if offset + write_len >= len {
                            bufpool.push(buf_index);

                            *token = Token::Poll { fd };

                            opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                                .build()
                                .user_data(token_index as _)
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

                            opcode::Write::new(types::Fd(fd), buf.as_ptr(), len as _)
                                .build()
                                .user_data(token_index as _)
                        };

                        unsafe {
                            if sq.push(&entry).is_err() {
                                backlog.push_back(entry);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn crate_client(port: u16, server_started: Arc<AtomicBool>) {
        //等服务端起来
        while !server_started.load(Ordering::Acquire) {}
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
        let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
            .unwrap_or_else(|_| panic!("connect to 127.0.0.1:3456 failed !"));
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

    #[test]
    fn original() -> anyhow::Result<()> {
        let port = 8488;
        let server_started = Arc::new(AtomicBool::new(false));
        let clone = server_started.clone();
        let handle = std::thread::spawn(move || crate_server(port, clone));
        std::thread::spawn(move || crate_client(port, server_started))
            .join()
            .expect("client has error");
        handle.join().expect("server has error")
    }

    pub fn crate_server2(port: u16, server_started: Arc<AtomicBool>) -> anyhow::Result<()> {
        let operator = IoUringOperator::new(0)?;
        let listener = TcpListener::bind(("127.0.0.1", port))?;

        let mut bufpool = Vec::with_capacity(64);
        let mut buf_alloc = Slab::with_capacity(64);
        let mut token_alloc = Slab::with_capacity(64);

        println!("listen {}", listener.local_addr()?);
        server_started.store(true, Ordering::Release);

        operator.accept(
            token_alloc.insert(Token::Accept),
            listener.as_raw_fd(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )?;

        loop {
            let mut r = operator.select(None)?;

            for cqe in &mut r.1 {
                let ret = cqe.result();
                let token_index = cqe.user_data() as usize;

                if ret < 0 {
                    eprintln!(
                        "token {:?} error: {:?}",
                        token_alloc.get(token_index),
                        io::Error::from_raw_os_error(-ret)
                    );
                    continue;
                }

                let token = &mut token_alloc[token_index];
                match token.clone() {
                    Token::Accept => {
                        println!("accept");

                        let fd = ret;
                        let poll_token = token_alloc.insert(Token::Poll { fd });

                        operator.poll_add(poll_token, fd, libc::POLLIN as _)?;
                    }
                    Token::Poll { fd } => {
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

                        operator.recv(token_index, fd, buf.as_mut_ptr() as _, buf.len(), 0)?;
                    }
                    Token::Read { fd, buf_index } => {
                        if ret == 0 {
                            bufpool.push(buf_index);
                            token_alloc.remove(token_index);
                            println!("shutdown connection");
                            unsafe { libc::close(fd) };

                            println!("Server closed");
                            return Ok(());
                        } else {
                            let len = ret as usize;
                            let buf = &buf_alloc[buf_index];

                            *token = Token::Write {
                                fd,
                                buf_index,
                                len,
                                offset: 0,
                            };

                            operator.send(token_index, fd, buf.as_ptr() as _, len, 0)?;
                        }
                    }
                    Token::Write {
                        fd,
                        buf_index,
                        offset,
                        len,
                    } => {
                        let write_len = ret as usize;

                        if offset + write_len >= len {
                            bufpool.push(buf_index);

                            *token = Token::Poll { fd };

                            operator.poll_add(token_index, fd, libc::POLLIN as _)?;
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

                            operator.write(token_index, fd, buf.as_ptr() as _, len)?;
                        };
                    }
                }
            }
        }
    }

    #[test]
    fn framework() -> anyhow::Result<()> {
        let port = 9898;
        let server_started = Arc::new(AtomicBool::new(false));
        let clone = server_started.clone();
        let handle = std::thread::spawn(move || crate_server2(port, clone));
        std::thread::spawn(move || crate_client(port, server_started))
            .join()
            .expect("client has error");
        handle.join().expect("server has error")
    }
}
