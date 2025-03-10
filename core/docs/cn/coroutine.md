---
title: 协程总览
date: 2024-12-29 16:00:00
author: loongs-zhang
---

# 协程总览

[English](../en/coroutine.md) | 中文

## 使用方法

```rust
use open_coroutine_core::common::constants::CoroutineState;
use open_coroutine_core::coroutine::Coroutine;

fn main() -> std::io::Result<()> {
    let mut co = Coroutine::new(
        // 可选的协程名称
        None,
        |suspender, input| {
            assert_eq!(1, input);
            assert_eq!(3, suspender.suspend_with(2));
            4
        },
        // 可选的栈大小
        None,
        // 可选的协程优先级
        None,
    )?;
    // 宏`co!`等同于上面的代码
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

## 什么是协程？

[协程](https://en.wikipedia.org/wiki/Coroutine)是一种可以暂停和恢复的函数，能够向调用者返回值。协程可以在其调用栈的任何位置挂起自己。除了从协程中接收返回值外，你还可以在每次恢复协程时向其传递数据。

以上内容摘自[corosensei](https://github.com/Amanieu/corosensei)。

## 协程 VS 线程

|         | 协程            | 线程     |
|---------|----------------|----------|
| 切换效率 | ✅ 更高         | ❌ 高    |
| 内存用量 | ✅ Bytes/KB/MB | ❌ KB/MB |
| 由OS调度 | ❌             | ✅       |
| 可伸缩栈 | ✅             | ❌       |

## 有栈协程 VS 无栈协程

|         | 有栈协程  | 无栈协程  |
|---------|----------|----------|
| 切换效率 | ❌ 高    | ✅ 更高   |
| 内存用量 | ❌ KB/MB | ✅ Bytes |
| 使用限制 | ✅ 较少   | ❌ 较多  |

一般来说，如果对资源利用率和切换性能的要求不是非常严格，使用有栈协程会更加方便，代码也更容易维护。因此，`open-coroutine`选择了有栈协程。

## open-coroutine中的状态

```text
           Ready
        ↗    ↓
Suspend ← Running ⇄ Syscall
           ↙   ↘
      Complete Error
```

在open-coroutine中，创建的协程处于`Ready`状态，一旦调用`Coroutine::resume_with`方法，状态将从`Ready`变为`Running`。之后，协程可能会通过`Suspender::suspend_with`挂起，状态将从`Running`变为`Suspend`，`Suspend`状态还会记录可以被唤醒的时间戳，单位为纳秒。

当协程进入系统调用时，协程状态将从`Running`变为`Syscall`，系统调用完成后，状态将从`Syscall`变回`Running`（注意：如果你使用[open-coroutine-core](https://crates.io/crates/open-coroutine-core)，你需要在适当的时候手动调用`Coroutine::syscall`和`Coroutine::running`来切换协程状态，这会增加大量工作量且容易出错，因此请使用[open-coroutine](https://crates.io/crates/open-coroutine)并启用`hook`）。此外，系统调用`Syscall`状态会记录系统调用的名称和一个用于open-coroutine内部的状态。

当协程成功完成时，状态将从`Running`变为`Complete`。如果在协程执行过程中发生panic且用户未处理该panic，状态将从`Running`变为`Error`同时记录panic信息。

## `Listener` 设计

To enhance extension, we provide the `Listener` API, which notifies `Listener` whenever Coroutine state changes.

为了增强扩展性，我们提供了`Listener`API，每当协程状态发生变化时，都会通知`Listener`。

## `CoroutineLocal` 设计

`ThreadLocal`的设计意图是解决多线程环境中的线程安全问题。在多线程程序中，多个线程可能同时访问和修改同一个共享变量，这可能导致线程安全问题，如数据不一致和竞态条件。为了解决这些问题，传统方法是通过锁来同步对共享资源的访问，但这可能导致性能下降，尤其是在高并发场景中。`ThreadLocal`提供了一种新的解决方案，它为每个线程提供变量的独立副本，从而避免了多个线程之间的共享变量冲突，确保了线程安全，并提高了程序的并发性能。

在协程环境中，尽管调度线程是单线程的，但由于工作窃取机制的存在，协程可能会在多个线程之间流动，这使得`ThreadLocal`失效。为了解决这个问题，我们不得不引入`CoroutineLocal`。它与`ThreadLocal`的副本提供方式类似，但`CoroutineLocal`将副本升级到了协程级别，这意味着每个协程都有自己的局部变量。这些局部变量将在协程被销毁时一起被丢弃。

## [可伸缩栈总览](scalable-stack.md)
