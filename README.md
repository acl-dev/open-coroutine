# open-coroutine

### What is open-coroutine ?
The `open-coroutine` is a simple, efficient and generic stackful-coroutine library.

### How to use this library ?

#### step1
add dependency to your `Cargo.toml`
```toml
[dependencies]
open-coroutine = "0.0.1"
```

#### step2 
enable hooks
```rust
fn main() {
    //step2 enable hooks
    open_coroutine::init();
    //......
}
```

#### step3 
enjoy the performance improvement brought by `open-coroutine`

### How to run examples ?
```shell
cargo run --example hello
```
