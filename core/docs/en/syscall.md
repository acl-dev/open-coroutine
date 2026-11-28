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

The `Iocp` layer exists only on Windows with `feature = "iocp"`, and on master it is
currently wired **only for `accept`/`WSAAccept`** (`IocpAcceptSyscall` in `accept.rs` →
`net::EventLoops::accept` → `Operator::acceptex`). All other Windows syscalls use
`Facade -> Nio -> Raw` even with `iocp` enabled; the `impl_iocp*` macros are defined but
unused. On Linux the equivalent slot is `io_uring` (`feature = "io_uring"`); on other
platforms the chain degrades to `Facade -> Nio -> Raw`.

## Platform backends

- **Linux `io_uring`**: file and socket syscalls are submitted as SQEs; completions are reaped from the CQ ring.
- **Windows `IOCP`**: currently only `accept`/`WSAAccept` are submitted as overlapped
  operations (`AcceptEx`) bound to a per-event-loop completion port. There is no
  overlapped-socket tracking yet, and `Operator::wsarecv`/`wsasend` still call the
  hooked `WSARecv`/`WSASend` entry points — a known hazard that must be resolved
  (e.g. via real-function-pointer registry) before the recv/send data plane can be
  wired. See [BUGS.md](../../../BUGS.md) B3–B6, B9.
- **NIO fallback**: used for every syscall without an `Iocp` impl, when the
  completion backend is unavailable, or when the caller supplies its own
  `lpoverlapped`/`lpcompletionroutine`.

## Return-value semantics

The hook must not change syscall semantics:

- `recv`/`send` return the transferred byte count; `WSARecv`/`WSASend` return 0 on success and write the byte count
  to `lpnumberofbytesrecvd`/`lpnumberofbytessent`; `accept`/`WSAAccept` return the new socket or `INVALID_SOCKET`.
- Errors return the platform's error sentinel (`-1`/`SOCKET_ERROR`/`INVALID_SOCKET`) with errno set.
- Overlapped submissions are transparent to the caller: instead of returning `SOCKET_ERROR + WSA_IO_PENDING`, the
  coroutine suspends and the final result is returned — the same contract the io_uring backend provides.

Syscalls whose success return is not a byte count need a hand-written `Iocp*` impl rather
than the `impl_iocp_read!`/`impl_iocp_write!` macros, which always return the transferred
count. On master the only such impl is `IocpAcceptSyscall` (`accept.rs`), which returns a
socket handle; `WSARecv`/`WSASend` have no `Iocp` impl yet.

## Hooked syscalls (Windows)

`Sleep`, `CreateFileW`, `SetFilePointerEx`, `accept`, `WSAAccept`, `connect`, `ioctlsocket`, `listen`, `recv`,
`send`, `shutdown`, `socket`, `setsockopt`, `WSARecv`, `WSASend`, `WSASocketW`, `select`, `WSAPoll`.

## Known limitations

- IOCP coverage is limited to `accept`/`WSAAccept`; all other socket and file syscalls
  use the NIO backend even with `feature = "iocp"`.
- `Operator::async_cancel` is `todo!("CancelIoEx")` — it panics if invoked.
- `connect` maps `WSAETIMEDOUT` back to `WSAEINPROGRESS` on timeout.
- `Operator::acceptex` has a live infinite-retry loop on non-`WSA_IO_PENDING` errors
  and a pending-buffer lifetime bug — see [BUGS.md](../../../BUGS.md) B3/B4.
