---
title: Why Better
date: 2025-01-10 08:28:00
author: loongs-zhang
---

# Why Better

## Syscall will not block

Firstly, let's take a look at how thread collaborate with syscall.

```mermaid
sequenceDiagram
    Actor User Thread
    participant Operation System

    User Thread ->>+ User Thread: execute
    alt User Thread blocked
    User Thread ->>+ Operation System: slow syscall
    Operation System ->> User Thread: return
    end
    User Thread ->>+ User Thread: execute
```

If the syscall is a slow syscall, such as `accept` without setting non-blocking, the thread will be blocked for a long
time and unable to do anything until the OS returns. Now, let's take a look at how open-coroutine collaborate with 
syscall.

```mermaid
sequenceDiagram
    Actor EventLoop Thread
    participant Coroutine1
    participant Coroutine2
    participant Hooked Syscall
    participant Operation System

    EventLoop Thread ->>+ Coroutine1: schedule
    alt Coroutine1 blocked logically
    Coroutine1 ->>+ Hooked Syscall: slow syscall
    Hooked Syscall ->>+ Operation System: fast syscall
    Operation System ->> Hooked Syscall: return errno
    Hooked Syscall ->> Coroutine1: suspend the coroutine for a period of time
    end
    Coroutine1 ->>+ EventLoop Thread: suspended
    EventLoop Thread ->>+ Coroutine2: schedule
    alt Coroutine2 blocked logically
    Coroutine2 ->>+ Hooked Syscall: slow syscall
    Hooked Syscall ->>+ Operation System: fast syscall
    Operation System ->> Hooked Syscall: return
    Hooked Syscall ->> Coroutine2: return
    end
    Coroutine2 ->>+ EventLoop Thread: return
    EventLoop Thread ->>+ Coroutine1: schedule
    alt Coroutine1 blocked logically
    Coroutine1 ->>+ Hooked Syscall: resume from the last pause
    Hooked Syscall ->>+ Operation System: fast syscall
    Operation System ->> Hooked Syscall: return
    Hooked Syscall ->> Coroutine1: return
    end
    Coroutine1 ->>+ EventLoop Thread: return
    EventLoop Thread ->>+ EventLoop Thread: schedule other coroutines
```

As you can see, `Hooked Syscall` converts `slow syscall` to `fast syscall`. In this way, although the `EventLoop Thread`
will still be blocked when executing syscall, the blocking time is very short. Therefore, compared to the thread model,
`EventLoop Thread` can do more things in the same amount of time.

## Heavy computing will not block

Secondly, let's take a look at how threads handle heavy computations.

```mermaid
sequenceDiagram
    Actor User Thread

    alt User Thread gets stuck in a loop
    User Thread ->>+ User Thread: execute loop
    end
```

Just like syscall above, thread will always block in the loop. Then, let's take a look at how open-coroutine handle 
heavy computations.

```mermaid
sequenceDiagram
    Actor EventLoop Thread
    participant Coroutine1
    participant Coroutine2
    participant Monitor

    EventLoop Thread ->>+ Coroutine1: schedule
    alt Coroutine1 enters loop
    Coroutine1 ->>+ Coroutine1: execute loop for a period of time
    Monitor ->> Coroutine1: suspend the coroutine
    end
    Coroutine1 ->>+ EventLoop Thread: suspended
    EventLoop Thread ->>+ Coroutine2: schedule
    alt Coroutine2 enters loop
    Coroutine2 ->>+ Coroutine2: execute loop for a period of time
    Monitor ->> Coroutine1: suspend the coroutine
    end
    Coroutine2 ->>+ EventLoop Thread: suspended
    EventLoop Thread ->>+ Coroutine1: schedule
    alt Coroutine1 enters loop
    Coroutine1 ->>+ Coroutine1: resume from the last pause
    end
    Coroutine1 ->>+ EventLoop Thread: return
    EventLoop Thread ->>+ EventLoop Thread: schedule other coroutines
```

`Monitor` will monitor the execution of coroutines, and once it found that the execution time of a coroutine is too 
long, it will force the coroutine to suspend. So now, we can even use just one `EventLoop Thread` to execute multiple 
loops, which cannot be achieved under the single threaded model.
