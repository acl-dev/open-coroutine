/// just for CI
pub fn init() {
    let _ = std::thread::spawn(|| {
        // exit after 600 seconds, just for CI
        std::thread::sleep(std::time::Duration::from_secs(600));
        std::process::exit(-1);
    });
}
