use open_coroutine_examples::{crate_co_client, crate_server};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[open_coroutine::main(event_loop_size = 2, max_size = 1)]
fn main() -> std::io::Result<()> {
    let port = 8899;
    let server_started = Arc::new(AtomicBool::new(false));
    let clone = server_started.clone();
    let server_finished_pair = Arc::new((Mutex::new(true), Condvar::new()));
    let server_finished = Arc::clone(&server_finished_pair);
    _ = std::thread::Builder::new()
        .name("crate_server".to_string())
        .spawn(move || crate_server(port, clone, server_finished_pair))
        .expect("failed to spawn thread");
    _ = std::thread::Builder::new()
        .name("crate_co_client".to_string())
        .spawn(move || crate_co_client(port, server_started))
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
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "The coroutine client did not completed within the specified time",
        ))
    } else {
        Ok(())
    }
}
