use open_coroutine::{maybe_grow, task};

#[open_coroutine::main(event_loop_size = 1, max_size = 1)]
pub fn main() {
    let join = task!(
        |_| {
            fn recurse(i: u32, p: &mut [u8; 10240]) {
                maybe_grow!(|| {
                    // Ensure the stack allocation isn't optimized away.
                    unsafe { _ = std::ptr::read_volatile(&p) };
                    if i > 0 {
                        recurse(i - 1, &mut [0; 10240]);
                    }
                })
                .expect("allocate stack failed")
            }
            println!("[coroutine] launched");
            // Use ~500KB of stack.
            recurse(50, &mut [0; 10240]);
            // Use ~500KB of stack.
            recurse(50, &mut [0; 10240]);
            println!("[coroutine] exited");
        },
        (),
    );
    assert_eq!(Some(()), join.join().expect("join failed"));
}
