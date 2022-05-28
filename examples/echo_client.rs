use std::io::Read;
use std::io::Write;
use std::net::TcpStream;

fn main() {
    let conn = TcpStream::connect("127.0.0.1:9898");
    match conn {
        Ok(mut stream) => {
            // 闭包
            let mut say = |txt: &[u8]| {
                let mut buf = [0u8; 1024];
                stream.write_all(txt).unwrap();
                let r = stream.read(&mut buf);
                if let Ok(size) = r {
                    let recv = &buf[0..size];
                    let recv_str = String::from_utf8_lossy(recv);
                    println!("Recv: {}", recv_str);
                }
            };
            say(b"Hello Server");
            say(b"Some message...");
            say(b"end bye~");
        }
        Err(e) => {
            println!("error->{}", e);
        }
    }
}