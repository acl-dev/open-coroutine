# libfiber-rs

### What is libfiber-rs ?
`libfiber-rs` is a rust wrapper of `lib_fiber` module in `acl`, it is a high-performance and lightweight network library with stack coroutine.

### How to use this library ?

#### step1 add dependency to your Cargo.toml
```toml
[dependencies]
libfiber = "0.1.0"
```

#### step2 enable hooks
```rust
use libfiber::hooks::Hooks;

fn main() {
    //step2 enable hooks
    Hooks::enable(true);
    //......
}
```