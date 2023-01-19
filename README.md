# open-coroutine

### What is open-coroutine ?
The `open-coroutine` is a simple, efficient and generic stackful-coroutine library.

### Status
Still under development, please `do not` use this library in the `production` environment !

Only support hook several `system calls`.

### Features
#### 0.1.0
- [x] basic suspend/resume supported
- [x] use jemalloc as memory pool
- [x] higher level coroutine abstraction supported
- [x] preemptive scheduling supported
- [x] work stealing supported
- [x] sleep system call hooks supported

### How to use this library ?

#### step1
add dependency to your `Cargo.toml`
```toml
[dependencies]
# check https://crates.io/crates/open-coroutine
open-coroutine = "x.y.z"
```

#### step2 
enable hooks
```rust
//step2 enable hooks
#[open_coroutine::main]
fn main() {
    //......
}
```

#### step3 
enjoy the performance improvement brought by `open-coroutine` !

### simplest example below
```rust
use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine1] launched");
            input
        },
        None,
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            input
        },
        None,
        4096,
    );
    std::thread::sleep(Duration::from_millis(50));
    println!("scheduler finished successfully!");
}
```

### preemptive example
Note: not supported for windows
```rust
use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    static mut FLAG: bool = true;
    let handle = co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine1] launched");
            unsafe {
                while FLAG {
                    println!("loop");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            input
        },
        Some(unsafe { std::mem::transmute(1usize) }),
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            unsafe {
                FLAG = false;
            }
            input
        },
        None,
        4096,
    );
    let result = handle.timeout_join(Duration::from_secs(1));
    assert_eq!(result.unwrap(), 1);
    unsafe { assert!(!FLAG) };
    println!("preemptive schedule finished successfully!");
}
```

### How to run examples ?
```shell
cargo run --example hello
cargo run --example preemptive
```
