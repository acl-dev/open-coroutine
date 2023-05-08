use open_coroutine_examples::sleep_test_co;

#[open_coroutine::main]
fn main() {
    sleep_test_co(1);
    sleep_test_co(1000);
}
