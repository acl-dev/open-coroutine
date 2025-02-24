---
title: Reason for Birth
date: 2025-02-24 17:08:33
author: loongs-zhang
---

# Reason for Birth

English | [中文](../cn/background.md)

## The thread pool needs to be optimized

In the early days, developers often adopted multiprocessing to support concurrent access to service applications by
multiple users, which creates a service process for each TCP connection. Around 2000, it was quite popular to use CGI to
write web services, and the most commonly used web server at that time was Apache 1.3.x series, which was developed
based on the multiprocessing model. Because processes occupy more system resources while threads occupy fewer resources,
people have started using multithreaded (usually using thread pools) to develop web service applications, which has
increased the user concurrency supported by a single server, but there is still a problem of resource waste.

In 2020, I joined the V company. Due to occasional occurrences of the thread pool being fully filled in the internal
system, coupled with the fact that the leader had
read [《Java线程池实现原理及其在美团业务中的实践》](https://tech.meituan.com/2020/04/02/java-pooling-pratice-in-meituan.html),
we decided to build our own dynamic thread pool. From the process, the results were good:

<div style="text-align: center;">
    <img src="/docs/img/begin.jpg" width="50%">
</div>

But this don't fundamentally solve the problem. As is well known, thread context switching has a certain cost, and the
more threads there are, the greater the cost of thread context switching. For CPU intensive tasks, simply ensure that
the number of threads is equal to the number of CPU cores and bind the threads to the specified CPU core (hereinafter
referred to as the `thread-per-core`), it can ensure optimal performance. For IO intensive tasks, since the task almost
always blocks threads, the cost of thread context switching is generally less than the blocking cost. However, when the
number of threads is too large, the cost of thread context switching will be greater than the blocking cost.

The essence of dynamic thread pool is to adjust the number of threads to minimize the cost of thread context switching
compared to blocking. Since this is manual, it cannot be guaranteed.

<div style="text-align: center;">
    <img src="/docs/img/run.jpg" width="50%">
</div>

## The pain of using NIO

Is there a technology that can perform IO intensive tasks with performance comparable to multithreading while ensuring
thread-per-core? The answer is `NIO`, but there are still some limitations or unfriendly aspects:

1. The NIO API is more complex to use compared to the BIO API;
2. System calls such as sleep still block threads. To achieve optimal performance, it is equivalent to disabling all
   blocking calls, which is unfriendly to developers;
3. In the thread pool mode, for a single thread, the next task can only be executed after the current task has been
   completed, which cannot achieve fair scheduling between tasks;

Note: Assuming a single thread with a CPU time slice of 1 second and 100 tasks, the fair scheduling refers to each task
being able to fairly occupy a 10ms time slice.

The first point can still be overcome, while the second and third points are weaknesses. In fact, if the third point can
be implemented, RPC frameworks don't need to have too many threads, just thread-per-core.

How can developers use it easily while ensuring that the performance of IO intensive tasks is not inferior to
multi threading and thread-per-core? The `Coroutine` technology slowly entered my field of vision.

## Goroutine still has shortcomings

At the beginning of playing with coroutines, due to the cost of learning, I first chose `kotlin`. However, when I
realized that kotlin's coroutines needed to change APIs (such as replacing Thread.sleep with kotlinx.coroutines.delay)
to avoid blocking threads, I decisively adjusted the direction to `golang`. About 2 weeks later:

<div style="text-align: center;">
    <img src="/docs/img/good.jpeg" width="50%">
</div>

Which technology is strong in coroutine? Look for Golang in program languages. However, as I delved deeper into my
studies, I discovered several shortcomings of goroutines:

1. `Not thread-per-core`. The goroutine runtime is also supported by a thread pool, and the maximum number of threads in
   this thread pool is 256, which is generally much larger than the number of threads in the thread-per-core, and the
   scheduling thread is not bound to the CPU;
2. `Preemptive scheduling will interrupt the running system calls`. If the system call takes a long time to complete, it
   will obviously be interrupted multiple times, resulting in a decrease in overall performance;
3. `There is a significant gap between goroutine and other in best performance`. Compared to the C/C++ coroutine
   library, its performance can even reach 1.5 times that of goroutines;

With regret, I continued to study the C/C++ coroutine libraries and found that they either only implemented `hook` (here
we explain hook technology, in simple terms, proxy system calls, such as calling sleep. Without the hook, the operating
system's sleep function would be called, and after the hook, it would point to our own code. For detailed operation
steps, please refer to Chapters 41 and 42 of The Linux Programming Interface), or only implemented `work-stealing`.
Some libraries only provided the most basic `coroutine abstraction`, and the most disappointing thing is that none of
then implemented `preemptive scheduling`.

There's no other way, it seems like we can only do it ourselves.

<div style="text-align: center;">
    <img src="/docs/img/just_do_it.jpg" width="100%">
</div>
