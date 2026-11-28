---
title: Syscall Dispatch
date: 2026-11-28 00:00:00
author: loongs-zhang
---

# Syscall Dispatch

English | [中文](../cn/syscall.md)

## Overview

`open-coroutine-hook` uses [minhook](https://github.com/TsudaKageyu/minhook) to intercept a fixed set of
slow syscalls (socket IO, sleep, file IO, see
[unix](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/unix.rs)/
[windows](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/windows.rs)). Each intercepted call
is forwarded to `open_coroutine_core::syscall::<name>(fn_ptr, args...)`, where `fn_ptr` is the trampoline to the
original function.

> **Note:** the hook cdylib statically links its own copy of `open-coroutine-core`, so the dispatch chain below runs
> inside the DLL's core instance — thread-local state is not shared with the executable's core.

## Dispatch chain

Every hooked syscall is dispatched through a four-layer chain (three layers when the platform backend is disabled):

```text
Facade -> Iocp -> Nio -> Raw        (windows + iocp)
Facade -> Nio   -> Raw              (otherwise)
```

| layer   | responsibility |
| ------- | -------------- |
| Facade  | coroutine state machine transitions (`Running -> Syscall(Executing)`), enter/exit logging, errno save/restore |
| Iocp    | submits overlapped I/O through `net::EventLoops` -> `EventLoop` -> `net::operator`, suspends the coroutine, waits for the IOCP completion |
| Nio     | non-blocking emulation: `set_non_blocking` + event-loop wait + retry loop |
| Raw     | invokes the real function via `fn_ptr` (minhook trampoline), or the FFI import when no hook is loaded |

The `Iocp` layer exists only on Windows with `feature = "iocp"`. On Linux the equivalent slot is `io_uring`
(`feature = "io_uring"`); on other platforms the chain degrades to `Facade -> Nio -> Raw`.

## Platform backends

- **Linux `io_uring`**: file and socket syscalls are submitted as SQEs; completions are reaped from the CQ ring.
- **Windows `IOCP`**: socket syscalls are submitted as overlapped operations (`WSARecvFrom`/`WSASendTo`/`AcceptEx`)
  bound to a per-event-loop completion port. Sockets must be created with `WSA_FLAG_OVERLAPPED`; the operator tracks
  them in `OVERLAPPED_SOCKETS`/`BOUND_SOCKETS` and rejects overlapped ops on foreign sockets so the caller falls back
  to NIO.
- **NIO fallback**: used when the completion backend is unavailable, the socket is not overlapped-capable, or the
  caller supplies its own `lpoverlapped`/`lpcompletionroutine`.

## Return-value semantics

The hook must not change syscall semantics:

- `recv`/`send` return the transferred byte count; `WSARecv`/`WSASend` return 0 on success and write the byte count
  to `lpnumberofbytesrecvd`/`lpnumberofbytessent`; `accept`/`WSAAccept` return the new socket or `INVALID_SOCKET`.
- Errors return the platform's error sentinel (`-1`/`SOCKET_ERROR`/`INVALID_SOCKET`) with errno set.
- Overlapped submissions are transparent to the caller: instead of returning `SOCKET_ERROR + WSA_IO_PENDING`, the
  coroutine suspends and the final result is returned — the same contract the io_uring backend provides.

Syscalls whose success return is not a byte count (e.g. `WSARecv`, `WSASend`, `accept`) use hand-written `Iocp*`
impls rather than the `impl_iocp_read!`/`impl_iocp_write!` macros, which always return the transferred count.

## Hooked syscalls (Windows)

`Sleep`, `CreateFileW`, `SetFilePointerEx`, `accept`, `WSAAccept`, `connect`, `ioctlsocket`, `listen`, `recv`,
`send`, `shutdown`, `socket`, `setsockopt`, `WSARecv`, `WSASend`, `WSASocketW`, `select`, `WSAPoll`.

## Known limitations

- `async_cancel` on the IOCP backend returns `Unsupported`.
- Caller-supplied `lpoverlapped`/`lpcompletionroutine` on `WSARecv`/`WSASend` bypasses IOCP and goes through the
  real asynchronous path via NIO.
- `connect` maps `WSAETIMEDOUT` back to `WSAEINPROGRESS` on timeout.
