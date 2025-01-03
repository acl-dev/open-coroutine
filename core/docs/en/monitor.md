---
title: Monitor Overview
date: 2025-01-02 08:35:00
author: loongs-zhang
---

# Monitor Overview

## Supported Targets

The `preemptive` feature currently supports the following targets:

|               | ELF (Linux, BSD, bare metal, etc) | Darwin (macOS, iOS, etc) | Windows |
|---------------|-----------------------------------|--------------------------|---------|
| `x86_64`      | ✅                                | ✅                       | ❌     |
| `x86`         | ✅                                | ❌                       | ❌     |
| `AArch64`     | ⚠️                                | ✅                       | ❌     |
| `ARM`         | ⚠️                                | ❌                       | ❌     |
| `RISC-V`      | ⚠️                                | ❌                       | ❌     |
| `LoongArch64` | ⚠️                                | ❌                       | ❌     |

✅ Tested and stable; ⚠️ Tested but unstable; ❌ Not supported.

## How it works

```mermaid
sequenceDiagram
    Actor User Thread
    participant Coroutine
    participant MonitorListener
    participant Monitor Thread

    User Thread ->>+ Coroutine: Coroutine::resume_with
    Coroutine ->>+ MonitorListener: Listener::on_state_changed
    MonitorListener ->>+ Monitor Thread: Monitor::submit
    Monitor Thread ->>+ Monitor Thread: libc::sigaction
    alt Preempting has occurred
        Coroutine ->> Coroutine: Resumed and the coroutine state is Running for more than 10ms
        Monitor Thread ->>+ User Thread: libc::pthread_kill
        User Thread ->>+ User Thread: libc::pthread_sigmask
        User Thread ->>+ Coroutine: suspend the coroutine, see sigurg_handler
        Coroutine ->> User Thread: coroutine has been preempted
    else No preempting
        Coroutine ->> Coroutine: The coroutine state changes to Suspend/Syscall/Complete/Error
        Coroutine ->>+ MonitorListener: Listener::on_state_changed
        MonitorListener ->>+ Monitor Thread: Monitor::remove
        Monitor Thread ->>+ MonitorListener: return
        MonitorListener ->>+ Coroutine: return
        Coroutine ->> User Thread: return
    end
```
