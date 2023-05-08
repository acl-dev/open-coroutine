use open_coroutine_examples::sleep_test;

#[open_coroutine::main]
fn main() {
    sleep_test(1);
    sleep_test(1000);
}
