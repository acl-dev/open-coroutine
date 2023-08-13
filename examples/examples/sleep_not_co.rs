use open_coroutine_examples::sleep_test;

#[open_coroutine::main(event_loop_size = 2, max_size = 2, keep_alive_time = 0)]
fn main() {
    sleep_test(1);
    sleep_test(1000);
}
