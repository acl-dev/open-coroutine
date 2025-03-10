---
title: Hook Overview
date: 2025-01-20 10:00:00
author: loongs-zhang
---

# Hook Overview

English | [中文](../cn/hook.md)

## Why hook?

After a `Coroutine::resume_with`, a coroutine may occupy the scheduling thread for a long time (e.g. getting stuck in
heavy computing or syscall), thereby slowing down other coroutines scheduled by that scheduling thread. To solve the
problem of getting stuck in syscall, we introduce hook, which automatically suspends coroutines that enter syscall and
allow other coroutines to execute.

This brings a new problem, the `preemptive` feature will send a large number of signals `which can interrupt the running
syscall`. In addition, most user code does not handle signals, if they directly use `open-routine-core` and enabling the
preemptive feature will lead to `catastrophic consequences`.

## What is hook?

Hook can modify or extend the behavior of existing code by inserting custom code at runtime, and even monitor,
intercept, modify, and redirect system calls. Now, let's use an [example](https://github.com/loongs-zhang/link-example)
to visually experience it.

Assuming we have the following test code:

```rust
use std::time::{Duration, Instant};

#[test]
fn test_hook() {
    let start = Instant::now();
    std::thread::sleep(Duration::MAX);
    let cost = Instant::now().duration_since(start);
    println!("cost: {:?}", cost);
}
```

If we don't hook, because `std::thread::sleep(Duration::MAX)`, this test almost never ends, but with hook, we redirect
the `nanosleep` syscall
to [our custom code](https://github.com/loongs-zhang/link-example/blob/master/dep/src/lib.rs) `without change the test
code`, and then the test
will [end soon](https://github.com/loongs-zhang/link-example/actions/runs/12862762378/job/35858206179).

<div style="text-align: center;">
    <img src="/hook/docs/img/result-on-macos.png" width="50%">
</div>

## How it works

```mermaid
sequenceDiagram
    Actor Your Project
    participant open-coroutine
    participant open-coroutine-hook
    participant open-coroutine-core
    
    Your Project ->> open-coroutine: depends on
    open-coroutine ->> open-coroutine-hook: depends on
    alt at compile time
        open-coroutine ->> open-coroutine: build open-coroutine-hook into dylib
        open-coroutine ->> open-coroutine: link open-coroutine-hook's dylib 
    else runtime
        Your Project -->> Operation System: logic execute syscall
        alt what actually happened
            Your Project ->> open-coroutine-hook: redirect syscall to open-coroutine-hook's syscall mod
            open-coroutine-hook ->> open-coroutine-core: call open-coroutine-core's syscall mod
            open-coroutine-core ->> Operation System: execute fast syscall actually
            Operation System ->> open-coroutine-core: return syscall result and errno
            open-coroutine-core -->> Operation System: maybe execute fast syscall many times
            open-coroutine-core -->> open-coroutine-core: maybe modify syscall result or errno
            open-coroutine-core ->> open-coroutine-hook: return syscall result and errno
            open-coroutine-hook ->> Your Project: return syscall result and errno
        end
        Operation System ->> Your Project: return syscall result and errno
    end
```
