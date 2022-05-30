use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

struct Count {
    inb: u64,
    outb: u64,
}

fn main() {
    let address = "127.0.0.1:9898";
    let package_length = 512;
    let clients = 500;
    let duration = 30;

    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let control = Arc::downgrade(&stop);
    for _ in 0..clients {
        let tx = tx.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            let mut sum = Count { inb: 0, outb: 0 };
            let mut out_buf: Vec<u8> = vec![0; package_length];
            out_buf[package_length - 1] = b'\n';
            let mut in_buf: Vec<u8> = vec![0; package_length];
            match TcpStream::connect(&*address) {
                Ok(mut stream)=>{
                    loop {
                        if (*stop).load(Ordering::Relaxed) {
                            break;
                        }

                        match stream.write_all(&out_buf) {
                            Err(_) => {
                                println!("Write error!");
                                break;
                            }
                            Ok(_) => sum.outb += 1,
                        }

                        if (*stop).load(Ordering::Relaxed) {
                            break;
                        }

                        match stream.read(&mut in_buf) {
                            Err(_) => break,
                            Ok(m) => {
                                if m == 0 || m != package_length {
                                    println!("Read error! length={}", m);
                                    break;
                                }
                            }
                        };
                        sum.inb += 1;
                    }
                },
                Err(e)=>println!("connect failed !")
            }
            tx.send(sum).unwrap();
        });
    }

    println!("client started !");
    thread::sleep(Duration::from_secs(duration));

    match control.upgrade() {
        Some(stop) => (*stop).store(true, Ordering::Relaxed),
        None => println!("Sorry, but all threads died already."),
    }

    let mut sum = Count { inb: 0, outb: 0 };
    for _ in 0..clients {
        let c: Count = rx.recv().unwrap();
        sum.inb += c.inb;
        sum.outb += c.outb;
    }
    println!("Benchmarking: {}", address);
    println!(
        "{} clients, running {} bytes, {} sec.",
        clients, package_length, duration
    );
    println!();
    println!(
        "Speed: {} request/sec, {} response/sec",
        sum.outb / duration,
        sum.inb / duration
    );
    println!("Requests: {}", sum.outb);
    println!("Responses: {}", sum.inb);
}