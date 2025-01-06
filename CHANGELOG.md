### 0.6.x

- [x] support custom task and coroutine priority.
- [x] support scalable stack

### 0.5.x

- [x] refactor syscall state, distinguish between state and innerState

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
- [x] sleep syscall hooks supported
