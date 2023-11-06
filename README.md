# open-coroutine

The `open-coroutine` is a simple, efficient and generic stackful-coroutine library.

<div style="text-align: center;">
    <img src="https://github.com/acl-dev/open-coroutine-docs/blob/master/img/architecture.png" width="100%">
</div>

[我有故事,你有酒吗?](https://github.com/acl-dev/open-coroutine-docs)

## Status

Still under development, please `do not` use this library in the `production` environment !

## How to use this library ?

### step1: add dependency to your Cargo.toml

```toml
[dependencies]
# check https://crates.io/crates/open-coroutine
open-coroutine = "x.y.z"
```

### step2: add macro

```rust
#[open_coroutine::main]
fn main() {
    //......
}
```

### step3: enjoy the performance improvement brought by open-coroutine !

## Examples

### Amazing preemptive schedule

Note: not supported for windows

```rust
#[open_coroutine::main]
fn main() -> std::io::Result<()> {
    cfg_if::cfg_if! {
        if #[cfg(all(unix, feature = "preemptive-schedule"))] {
            use open_coroutine_core::scheduler::Scheduler;
            use std::sync::{Arc, Condvar, Mutex};
            use std::time::Duration;

            static mut TEST_FLAG1: bool = true;
            static mut TEST_FLAG2: bool = true;
            let pair = Arc::new((Mutex::new(true), Condvar::new()));
            let pair2 = Arc::clone(&pair);
            let handler = std::thread::Builder::new()
                .name("preemptive".to_string())
                .spawn(move || {
                    let scheduler = Scheduler::new();
                    _ = scheduler.submit(
                        |_, _| {
                            println!("coroutine1 launched");
                            while unsafe { TEST_FLAG1 } {
                                println!("loop1");
                                _ = unsafe { libc::usleep(10_000) };
                            }
                            println!("loop1 end");
                            1
                        },
                        None,
                    );
                    _ = scheduler.submit(
                        |_, _| {
                            println!("coroutine2 launched");
                            while unsafe { TEST_FLAG2 } {
                                println!("loop2");
                                _ = unsafe { libc::usleep(10_000) };
                            }
                            println!("loop2 end");
                            unsafe { TEST_FLAG1 = false };
                            2
                        },
                        None,
                    );
                    _ = scheduler.submit(
                        |_, _| {
                            println!("coroutine3 launched");
                            unsafe { TEST_FLAG2 = false };
                            3
                        },
                        None,
                    );
                    scheduler.try_schedule();

                    let (lock, cvar) = &*pair2;
                    let mut pending = lock.lock().unwrap();
                    *pending = false;
                    // notify the condvar that the value has changed.
                    cvar.notify_one();
                })
                .expect("failed to spawn thread");

            // wait for the thread to start up
            let (lock, cvar) = &*pair;
            let result = cvar
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_millis(3000),
                    |&mut pending| pending,
                )
                .unwrap();
            if result.1.timed_out() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "preemptive schedule failed",
                ))
            } else {
                unsafe {
                    handler.join().unwrap();
                    assert!(!TEST_FLAG1);
                }
                Ok(())
            }
        } else {
            println!("please enable preemptive-schedule feature");
            Ok(())
        }
    }
}
```

outputs

```text
coroutine1 launched
loop1
coroutine2 launched
loop2
coroutine3 launched
loop1
loop2 end
loop1 end
```

### Arbitrary use of blocking syscalls

```rust
#[open_coroutine::main]
fn main() {
    std::thread::sleep(std::time::Duration::from_secs(1));
}
```

outputs

```text
nanosleep hooked
```

## Features

### todo

- [ ] refactor syscall state, distinguish between state and innerState
- [ ] Support and compatibility for AF_XDP socket
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
- [ ] support muti low_level coroutine create (just support [boost.context](https://github.com/boostorg/context) for
  now)
- [ ] support `#[open_coroutine::join]` macro to wait coroutines

### 0.4.x

- [x] Supports and is compatible with io_uring in terms of local file IO
- [x] elegant shutdown
- [x] use log instead of println
- [x] enhance `#[open_coroutine::main]` macro
- [x] refactor hook impl, no need to publish dylibs now
- [x] `Monitor` follow the `thread-per-core` guideline
- [x] `EventLoop` follow the `thread-per-core` guideline

### 0.3.x

- [x] ~~support `genawaiter` as low_level stackless coroutine (can't support it due to hook)~~
- [x] use `corosensei` as low_level coroutine
- [x] support backtrace
- [x] support `#[open_coroutine::co]` macro
- [x] refactor `WorkStealQueue`

### 0.2.x

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

### 0.1.x

- [x] basic suspend/resume supported
- [x] use jemalloc as memory pool
- [x] higher level coroutine abstraction supported
- [x] preemptive scheduling supported
- [x] work stealing supported
- [x] sleep system call hooks supported
