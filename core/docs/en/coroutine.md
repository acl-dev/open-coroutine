---
title: Coroutine Overview
date: 2024-12-29 16:00:00
author: loongs-zhang
---

# Coroutine Overview

## Usage

```rust
use open_coroutine_core::common::constants::CoroutineState;
use open_coroutine_core::coroutine::Coroutine;

fn main() -> std::io::Result<()> {
    let mut co = Coroutine::new(
        // optional coroutine name
        None,
        |suspender, input| {
            assert_eq!(1, input);
            assert_eq!(3, suspender.suspend_with(2));
            4
        },
        // optional stack size
        None,
        // optional coroutine priority
        None,
    )?;
    // Macro `co!` is equivalent to the code above
    // let mut co = open_coroutine_core::co!(|suspender, input| {
    //     assert_eq!(1, input);
    //     assert_eq!(3, suspender.suspend_with(2));
    //     4
    // })?;
    assert_eq!(CoroutineState::Suspend(2, 0), co.resume_with(1)?);
    assert_eq!(CoroutineState::Complete(4), co.resume_with(3)?);
    Ok(())
}
```

## What is coroutine?

A [coroutine](https://en.wikipedia.org/wiki/Coroutine) is a function that can be paused and resumed, yielding values to
the caller. A coroutine can suspend itself from any point in its call stack. In addition to receiving yielded values
from a coroutine, you can also pass data into the coroutine each time it is
resumed.

The above is excerpted from [corosensei](https://github.com/Amanieu/corosensei).

## Coroutine VS Thread

|                   | coroutine      | thread   |
|-------------------|----------------|----------|
| switch efficiency | ✅ Higher      | ❌ High  |
| memory usage      | ✅ Bytes/KB/MB | ❌ KB/MB |
| scheduled by OS   | ❌             | ✅       |
| stack grow        | ✅             | ❌       |

## Stackfull VS Stackless

|                   | stackfull | stackless |
|-------------------|-----------|-----------|
| switch efficiency | ❌ High   | ✅ Higher |
| memory usage      | ❌ KB/MB  | ✅ Bytes  |
| limitations       | ✅ Few    | ❌ Many   |

In general, if the requirements for resource utilization and switching performance are not very strict, using a
stackfull approach would be more convenient and the code would be easier to maintain. So, `open-coroutine` chooses the
stackfull coroutine.

## State in open-coroutine

```text
           Ready
        ↗    ↓
Suspend ← Running ⇄ Syscall
           ↙   ↘
      Complete Error
```

In open-coroutine, a coroutine created is in `Ready` state, once you call the `Coroutine::resume_with` method, the state
will change from `Ready` to `Running`. After that, the coroutine maybe suspend by `Suspender::delay_with`, then the
state will change from `Running` to `Suspend`, the `Suspend` state also records the timestamp that can be awakened, and
its unit is ns.

When the coroutine enters a syscall, the state will change from `Running` to `Syscall`, and after the syscall is
completed, the state will change from `Syscall` to `Running`(Note: if you
use [open-coroutine-core](https://crates.io/crates/open-coroutine-core), you will need to manually switch the coroutine
state by calling `Coroutine::syscall` and `Coroutine::running` at the appropriate time, which is a huge workload and
prone to errors, so please use [open-coroutine](https://crates.io/crates/open-coroutine) and enable `hook`). BTW,
the `Syscall` state records the syscall name and a helpful state which used in open-coroutine inner.

When the coroutine is successfully completed, the state will change from `Running` to `Complete`. If an error occurs
during the coroutine execution, and the coroutine does not handle the error, the state will change from `Running`
to `Error`, the error message will be recorded at the same time.

## `Listener` Design

To enhance extension, we provide the `Listener` API, which notifies `Listener` whenever Coroutine state changes.

## `CoroutineLocal` Design

The original design intention of `ThreadLocal` is to solve thread safety issues in multi-thread environments. In
multi-thread programs, multiple threads may simultaneously access and modify the same shared variable, which can lead
to thread safety issues such as data inconsistency and race conditions. To solve these issues, the traditional approach
is to synchronize access to shared resources through locking, but this can lead to performance degradation, especially
in high concurrency scenarios. `ThreadLocal` provides a new solution by providing independent copies of variables for each
thread, thereby avoiding shared variable conflicts between multiple threads, ensuring thread safety, and improving
program concurrency performance.

In the coroutine environment, although the scheduled thread is single, due to the existence of the work steel mechanism,
coroutines may flow between multiple threads which makes `ThreadLocal` invalid. To solve this problem, we have to
introduce `CoroutineLocal`. It's similar to `ThreadLocal`'s approach of providing replicas, but `CoroutineLocal` has
upgraded the replicas to the coroutine level, which means each coroutine has its own local variables. These local
variables will be dropped together when the coroutine is dropped.

## [Scalable Stack Overview](scalable-stack.md)
