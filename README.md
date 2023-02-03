# open-coroutine

### What is open-coroutine ?
The `open-coroutine` is a simple, efficient and generic stackful-coroutine library.

### Status
Still under development, please `do not` use this library in the `production` environment !

### Features
#### todo
- [ ] hook other syscall maybe interrupt by signal
  <details>
  <summary>syscalls</summary>

    - [ ] open
    - [ ] chdir
    - [ ] chroot
    - [ ] mkdir
    - [ ] rmdir
    - [ ] link
    - [ ] unlink
    - [ ] readlink
    - [ ] stat
    - [ ] dup
    - [ ] dup2
    - [ ] umask
    - [ ] mount
    - [ ] umount
    - [ ] mknod
    - [ ] fcntl
    - [ ] truncate
    - [ ] ftruncate
    - [ ] setjmp
    - [ ] longjmp
    - [ ] chown
    - [ ] lchown
    - [ ] fchown
    - [ ] chmod
    - [ ] fchmod
    - [ ] fchmodat
    - [ ] semop
    - [ ] ppoll
    - [ ] pselect
    - [ ] io_getevents
    - [ ] semop
    - [ ] semtimedop
    - [ ] msgrcv
    - [ ] msgsnd

  </details>

- [ ] consider use `corosensei` as low_level coroutine
- [ ] support back trace
- [ ] support `#[open_coroutine::join]` macro to wait coroutines
- [x] support `#[open_coroutine::co]` macro
- [x] refactor `WorkStealQueue` to singleton
- [ ] optimize `Stack` and `OpenCoroutine` to make `cache miss` happen less
- [ ] `Monitor` follow the `thread-per-core` guideline
- [ ] `EventLoop` follow the `thread-per-core` guideline, don't forget to consider the `Monitor` thread

#### 0.2.0
- [x] use correct `epoll_event` struct
- [x] use `rayon` for parallel computing
- [x] support `#[open_coroutine::main]` macro
- [x] hook almost all `read` syscall
  <details>
  <summary>read syscalls</summary>
  
  - [x] recv
  - [x] readv
  - [x] pread
  - [x] preadv
  - [x] recvfrom
  - [x] recvmsg

  </details>

- [x] hook almost all `write` syscall
  <details>
  <summary>write syscalls</summary>

  - [x] send
  - [x] write
  - [x] writev
  - [x] sendto
  - [x] sendmsg
  - [x] pwrite
  - [x] pwritev

  </details>

- [x] hook other syscall
  <details>
  <summary>other syscalls</summary>
  
  - [x] sleep
  - [x] usleep
  - [x] nanosleep
  - [x] connect
  - [x] listen
  - [x] accept
  - [x] shutdown
  - [x] poll
  - [x] select

  </details>

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

### examples
#### simplest example

run hello example
```shell
cargo run --example hello
```

<details>
<summary>Click to see code</summary>

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

</details>

#### preemptive example

Note: not supported for windows

run preemptive example
```shell
cargo run --example preemptive
```

<details>
<summary>Click to see code</summary>

```rust
use open_coroutine::co;
use std::os::raw::c_void;
use std::time::Duration;

#[open_coroutine::main]
fn main() {
    static mut EXAMPLE_FLAG: bool = true;
    let handle = co(
        |_yielder, input: Option<&'static mut i32>| {
            println!("[coroutine1] launched");
            unsafe {
                while EXAMPLE_FLAG {
                    println!("loop");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            input
        },
        Some(Box::leak(Box::new(1))),
        4096,
    );
    co(
        |_yielder, input: Option<&'static mut c_void>| {
            println!("[coroutine2] launched");
            unsafe {
              EXAMPLE_FLAG = false;
            }
            input
        },
        None,
        4096,
    );
    let result = handle.join();
    unsafe {
        assert_eq!(std::ptr::read_unaligned(result.unwrap() as *mut i32), 1);
        assert!(!EXAMPLE_FLAG);
    }
    unsafe { assert!(!EXAMPLE_FLAG) };
    println!("preemptive schedule finished successfully!");
}
```

</details>
