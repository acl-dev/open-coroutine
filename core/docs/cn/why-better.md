---
title: 为什么更好
date: 2025-01-10 08:28:00
author: loongs-zhang
---

# 为什么更好

[English](../en/why-better.md) | 中文

## 系统调用不会阻塞

首先，我们来看一下线程是如何与系统调用协作的。

```mermaid
sequenceDiagram
    actor 用户线程
    participant 操作系统
    用户线程 ->>+ 用户线程: 执行
    alt 用户线程被阻塞
        用户线程 ->>+ 操作系统: 慢系统调用
        操作系统 ->> 用户线程: 返回
    end
    用户线程 ->>+ 用户线程: 执行
```

如果系统调用是一个慢系统调用，例如默认阻塞的`accept`，线程将被长时间阻塞，直到操作系统返回为止，期间无法做任何事情。现在，我们来看一下 open-coroutine 是如何与系统调用协作的。

```mermaid
sequenceDiagram
    actor EventLoop线程
    participant 协程1
    participant 协程2
    participant 被代理的系统调用
    participant 操作系统
    EventLoop线程 ->>+ 协程1: 调度
    alt 协程1逻辑上被阻塞
        协程1 ->>+ 被代理的系统调用: 慢系统调用
        被代理的系统调用 ->>+ 操作系统: 快系统调用
        操作系统 ->> 被代理的系统调用: 返回错误码
        被代理的系统调用 ->> 协程1: 挂起协程一段时间
    end
    协程1 ->>+ EventLoop线程: 挂起
    EventLoop线程 ->>+ 协程2: 调度
    alt 协程2逻辑上被阻塞
        协程2 ->>+ 被代理的系统调用: 慢系统调用
        被代理的系统调用 ->>+ 操作系统: 快系统调用
        操作系统 ->> 被代理的系统调用: 返回
        被代理的系统调用 ->> 协程2: 返回
    end
    协程2 ->>+ EventLoop线程: 返回
    EventLoop线程 ->>+ 协程1: 调度
    alt 协程1逻辑上被阻塞
        协程1 ->>+ 被代理的系统调用: 从上次暂停处恢复
        被代理的系统调用 ->>+ 操作系统: 快系统调用
        操作系统 ->> 被代理的系统调用: 返回
        被代理的系统调用 ->> 协程1: 返回
    end
    协程1 ->>+ EventLoop线程: 返回
    EventLoop线程 ->>+ EventLoop线程: 调度其他协程
```

如你所见，`被代理的系统调用`(hook)将`慢系统调用`转换为`快系统调用`。通过这种方式，尽管`EventLoop线程`在执行系统调用时仍然会被阻塞，但阻塞时间非常短。因此，与线程模型相比，`EventLoop线程`可以在相同的时间内做更多的事情。

## 重度计算不会阻塞

其次，我们来看一下线程如何处理重度计算。

```mermaid
sequenceDiagram
    actor 用户线程
    alt 用户线程陷入循环
        用户线程 ->>+ 用户线程: 执行循环
    end
```

就像上面的系统调用一样，线程会一直阻塞在循环中。接下来，我们来看一下open-coroutine如何处理重度计算。

```mermaid
sequenceDiagram
    actor EventLoop线程
    participant 协程1
    participant 协程2
    participant Monitor
    EventLoop线程 ->>+ 协程1: 调度
    alt 协程1进入循环
        协程1 ->>+ 协程1: 执行循环一段时间
        Monitor ->> 协程1: 挂起协程
    end
    协程1 ->>+ EventLoop线程: 挂起
    EventLoop线程 ->>+ 协程2: 调度
    alt 协程2进入循环
        协程2 ->>+ 协程2: 执行循环一段时间
        Monitor ->> 协程1: 挂起协程
    end
        协程2 ->>+ EventLoop线程: 挂起
        EventLoop线程 ->>+ 协程1: 调度
    alt 协程1进入循环
        协程1 ->>+ 协程1: 从上次暂停处恢复
    end
    协程1 ->>+ EventLoop线程: 返回
    EventLoop线程 ->>+ EventLoop线程: 调度其他协程
```

`Monitor`会监控协程的执行情况，一旦发现某个协程的执行时间过长，就会强制挂起该协程。因此，现在我们甚至可以[使用一个`EventLoop线程`来执行多个循环](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/preemptive.rs)，这是单线程模型无法实现的。
