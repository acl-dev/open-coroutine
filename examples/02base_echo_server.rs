use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    loop {
        let r = stream.read(&mut buf);
        if let Ok(size) = r {
            if size <= 0 {
                return;
            }
            let recv = &buf[0..size];
            let recv_str = String::from_utf8_lossy(recv);
            println!("{}", recv_str);
            let res = stream.write_all(recv);
            match res {
                Ok(_) => {}
                Err(_) => {
                    return;
                }
            }
            if recv_str.starts_with("end") {
                println!("End tcp");
                return;
            }
        }
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:9999");
    println!("server started !");
    match listener {
        Ok(listener) => {
            for sr in listener.incoming() {
                match sr {
                    Ok(stream) => {
                        // 起新线程处理，move表示移交所有权
                        thread::spawn(move || {
                            handle_client(stream);
                        });
                    }
                    Err(_) => {}
                }
            }
        }
        Err(e) => {
            println!("error->{}", e);
        }
    }
}