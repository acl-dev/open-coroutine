use open_coroutine_examples::{crate_co_client, crate_co_server};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let port = 8999;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
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
