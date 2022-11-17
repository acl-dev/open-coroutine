# open-coroutine

### What is open-coroutine ?
The `open-coroutine` is a simple, efficient and generic stackful-coroutine library.

### Status
Still under development, please `do not` use this library in the `production` environment !

Only support hook several `system calls`.

Windows support is on the way, but the `priority is low`.

### How to use this library ?

#### step1
add dependency to your `Cargo.toml`
```toml
[dependencies]
open-coroutine = "0.0.7"
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
enjoy the performance improvement brought by `open-coroutine` !

### simplest example below
```rust
use open_coroutine::{co, Yielder};
use std::os::raw::c_void;
use std::time::Duration;

extern "C" fn f1(
    _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    _input: Option<&'static mut c_void>,
) -> Option<&'static mut c_void> {
    println!("[coroutine1] launched");
    None
}

extern "C" fn f2(
    _yielder: &Yielder<Option<&'static mut c_void>, (), Option<&'static mut c_void>>,
    _input: Option<&'static mut c_void>,
) -> Option<&'static mut c_void> {
    println!("[coroutine2] launched");
    None
}

fn main() {
    // because we used open_coroutine::co()
    // we don't need to call open_coroutine::init()
    // otherwise, don't forget open_coroutine::init()
    co(f1, None, 4096);
    co(f2, None, 4096);
    std::thread::sleep(Duration::from_millis(1));
    println!("scheduler finished successfully!");
}
```

### How to run examples ?
```shell
cargo run --example hello
```
