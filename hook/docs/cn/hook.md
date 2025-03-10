---
title: Hook总览
date: 2025-01-20 10:00:00
author: loongs-zhang
---

# Hook总览

[English](../en/hook.md) | 中文

## 为什么hook?

在`Coroutine::resume_with`之后，一个协程可能会长时间占用调度线程(例如，陷入重度计算或系统调用)，从而拖慢被该线程调度的其他协程。为了解决陷入系统调用的问题，我们引入hook机制，这样当协程进入系统调用时，它会被自动挂起，从而让其他协程执行。

这带来了一个新问题，`preemptive`特性会`发送大量信号`，而`信号会中断正在执行的系统调用`。此外，由于大多数用户在代码中未处理信号，因此如果他们直接使用`open-routine-core`并且启用`preemptive`特性将导致`灾难性后果`。

## 什么是hook?

Hook可以通过在运行时插入自定义代码来修改或扩展现有代码的行为，甚至可以监控、拦截、修改和重定向系统调用。现在，让我们用一个[例子](https://github.com/loongs-zhang/link-example)来直观地体验它。

假设我们有以下测试代码：

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

如果我们不hook，因为`std::thread::sleep(Duration::MAX)`，这个测试几乎永远不会结束，但有了hook，我们可以在`不更改测试代码`的情况下将`nanosleep`系统调用重定向到[我们的自定义代码](https://github.com/loongs-zhang/link-example/blob/master/dep/src/lib.rs)，然后测试就会[很快结束](https://github.com/loongs-zhang/link-example/actions/runs/12862762378/job/35858206179)。

<div style="text-align: center;">
    <img src="/hook/docs/img/result-on-macos.png" width="50%">
</div>

## 工作原理

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
