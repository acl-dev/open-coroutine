---
title: Coroutine Pool Overview
date: 2025-01-18 10:00:00
author: loongs-zhang
---

# 协程池总览

[English](../en/coroutine-pool.md) | 中文

## 使用方法

```rust
use open_coroutine_core::co_pool::CoroutinePool;

fn main() -> std::io::Result<()> {
    let mut pool = CoroutinePool::default();
    assert!(pool.is_empty());
    pool.submit_task(
        None,
        |_| {
            println!("Hello, world!");
            Some(2)
        },
        None,
        None,
    )?;
    assert!(!pool.is_empty());
    pool.try_schedule_task()
}
```

## 为什么需要协程池？

使用协程池可以带来以下几个显著的优势：

1. 资源管理：协程池可以管理协程的创建、销毁和复用。通过使用协程池，可以预先创建一定数量的协程并存储在池中，以便在需要时使用。这样可以避免频繁创建和销毁协程，减少不必要的资源浪费，并提高系统性能。

2. 避免协程饥饿：在使用协程池时，协程会持续获得任务，避免了协程在完成任务后空闲的情况。

3. 并发控制：通过设置协程池的参数，可以限制并发协程的数量，避免因协程过多而导致系统过载。

4. 提高代码可维护性：使用协程池可以将任务执行与协程管理分离，使代码更加清晰和易于维护。任务的执行逻辑可以专注于任务本身，而协程的创建和管理则由协程池处理。

## 工作原理

在open-coroutine-core中，协程池是惰性的，这意味着如果你不调用`CoroutinePool::try_timeout_schedule_task`，任务将不会被执行。详情请参考以下时序图：

```mermaid
sequenceDiagram
    actor Schedule Thread
    participant CoroutinePool
    participant WorkerCoroutine
    participant Task
    participant CoroutineCreator
    Schedule Thread ->>+ CoroutinePool: CoroutinePool::try_timeout_schedule_task
    alt 协程池已停止
        CoroutinePool ->>+ Schedule Thread: 返回错误
    end
    alt 协程池中的任务队列为空
        CoroutinePool ->>+ Schedule Thread: 返回成功
    end
    alt 创建工作协程
        CoroutinePool ->>+ WorkerCoroutine: 仅在协程池未达到最大池大小时创建工作协程
    end
    CoroutinePool ->>+ WorkerCoroutine: 调度工作协程
    alt 运行任务
        WorkerCoroutine ->>+ Task: 尝拉取任务
        alt 拉取成功
            Task ->>+ Task: 运行任务
            alt 执行中
                Task ->>+ WorkerCoroutine: 被抢占或进入系统调用
                WorkerCoroutine ->>+ WorkerCoroutine: 协程状态变为Suspend/Syscall
                WorkerCoroutine ->>+ CoroutineCreator: Listener::on_state_changed
                CoroutineCreator ->>+ WorkerCoroutine: 仅在协程池未达到最大池大小时创建工作协程
            end
            alt 运行成功
                Task ->>+ WorkerCoroutine: 任务正常退出
            end
            alt 运行失败
                Task ->>+ WorkerCoroutine: 任务异常退出
                WorkerCoroutine ->>+ WorkerCoroutine: 协程状态变为Error
                WorkerCoroutine ->>+ CoroutineCreator: Listener::on_state_changed
                CoroutineCreator ->>+ CoroutineCreator: 减少当前协程计数
                CoroutineCreator ->>+ WorkerCoroutine: 仅在协程池未达到最大池大小时重新创建工作协程
            end
        end
        alt 拉取失败
            Task ->>+ WorkerCoroutine: 增加拉取失败计数并让出给下一个协程
            WorkerCoroutine ->>+ WorkerCoroutine: 如果拉取失败计数已达到协程池的当前大小，则阻塞当前线程一段时间
        end
        WorkerCoroutine ->>+ WorkerCoroutine: 尝试拉取下一个任务
    end
    alt 回收协程
        WorkerCoroutine ->>+ WorkerCoroutine: 本次调度达到超时时刻
        WorkerCoroutine ->>+ CoroutinePool: 协程池是否超过最小池大小？
        CoroutinePool ->>+ WorkerCoroutine: 是
        WorkerCoroutine ->>+ WorkerCoroutine: 退出
    end
    WorkerCoroutine ->>+ CoroutinePool: 如果超时或调度失败则返回
    CoroutinePool ->>+ Schedule Thread: 本次调度结束
    Schedule Thread ->>+ Schedule Thread: ......
```
