---
title: Coroutine Pool Overview
date: 2025-01-18 10:00:00
author: loongs-zhang
---

# Coroutine Pool Overview

## Usage

```rust
use open_coroutine_core::co_pool::CoroutinePool;

fn main() -> std::io::Result<()> {
    let mut pool = CoroutinePool::default();
    assert!(pool.is_empty());
    pool.submit_task(
        Some(String::from(task_name)),
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

## Why coroutine pool?

Pooling the coroutines can bring several significant advantages:

1. Resource management: The coroutine pool can manage the creation, destruction, and reuse of coroutines. By using a
   coroutine pool, a certain number of coroutines can be created in advance and stored in the pool for use when needed.
   This can avoid frequent creation and destruction of coroutines, reduce unnecessary resource waste, and improve system
   performance.

2. Avoid coroutine hunger: When using a coroutine pool, coroutines will be continuously provided with tasks, avoiding
   the situation where coroutines are idle after completing tasks.

3. Concurrency control: By setting the parameters of the coroutine pool, the number of concurrent coroutines can be
   limited to avoid overloading the system due to too many coroutines.

4. Improve code maintainability: Using coroutine pools can separate task execution from coroutine management, making the
   code clearer and more maintainable. The execution logic of a task can be focused on the task itself, while the
   creation and management of coroutines are handled by the coroutine pool.

## How it works

In open-coroutine-core, the coroutine pool is lazy, which means if you don't call `try_timeout_schedule_task`, tasks
will not be executed. Please refer to the sequence diagram below for details:

```mermaid
sequenceDiagram
    Actor Schedule Thread
    participant CoroutinePool
    participant WorkerCoroutine
    participant Task
    participant CoroutineCreator

    Schedule Thread ->>+ CoroutinePool: CoroutinePool::try_timeout_schedule_task
    alt the coroutine pool is stopped
        CoroutinePool ->>+ Schedule Thread: return error
    end
    alt the task queue in the coroutine pool is empty
        CoroutinePool ->>+ Schedule Thread: return success
    end
    alt create worker coroutines
        CoroutinePool ->>+ WorkerCoroutine: create worker coroutines only if the coroutine pool has not reached its maximum pool size
    end
    CoroutinePool ->>+ WorkerCoroutine: schedule the worker coroutines
    alt run tasks
        WorkerCoroutine ->>+ Task: try poll a task
        alt poll success
            Task ->>+ Task: run the task
            alt in execution
                Task ->>+ WorkerCoroutine: be preempted or enter syscall
                WorkerCoroutine ->>+ WorkerCoroutine: The coroutine state changes to Suspend/Syscall
                WorkerCoroutine ->>+ CoroutineCreator: Listener::on_state_changed
                CoroutineCreator ->>+ WorkerCoroutine: create worker coroutines only if the coroutine pool has not reached its maximum pool size
            end
            alt run success
                Task ->>+ WorkerCoroutine: Task exited normally
            end
            alt run fail
                Task ->>+ WorkerCoroutine: Task exited abnormally
                WorkerCoroutine ->>+ WorkerCoroutine: The coroutine state changes to Error
                WorkerCoroutine ->>+ CoroutineCreator: Listener::on_state_changed
                CoroutineCreator ->>+ CoroutineCreator: reduce the current coroutine count
                CoroutineCreator ->>+ WorkerCoroutine: recreate worker coroutine only if the coroutine pool has not reached its maximum pool size
            end
        end
        alt poll fail
            Task ->>+ WorkerCoroutine: increase count and yield to the next coroutine
            WorkerCoroutine ->>+ WorkerCoroutine: block for a while if the count has reached the current size of coroutine pool
        end
        WorkerCoroutine ->>+ WorkerCoroutine: try poll the next task
    end
    alt recycle coroutines
        WorkerCoroutine ->>+ WorkerCoroutine: the schedule has exceeded the timeout time
        WorkerCoroutine ->>+ CoroutinePool: has the coroutine pool exceeded the minimum pool size?
        CoroutinePool ->>+ WorkerCoroutine: yes
        WorkerCoroutine ->>+ WorkerCoroutine: exit
    end
    WorkerCoroutine ->>+ CoroutinePool: return if timeout or schedule fail
    CoroutinePool ->>+ Schedule Thread: This schedule has ended
    Schedule Thread ->>+ Schedule Thread: ......
```
