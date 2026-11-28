# BUGS.md — open-coroutine bug & known-issue ledger

> Bugs and known gaps only. Features/design ideas go elsewhere.
> Statuses: **open** / **reproducing** / **fixed (unverified)** / **fixed (verified)** / **wontfix**.
> Update this file in the same commit that fixes an entry.
> Locations use `Type::method` / `module::function` anchors, never bare line numbers.

Ledger reflects **master** (`61410c5`). Several entries already have fixes on
the unmerged `iocp20261128` branch — check there before re-deriving them.

## Open

| # | Severity | Status | Location | Symptom / impact | Notes / fix direction |
|---|----------|--------|----------|------------------|-----------------------|
| B1 | medium | reproducing | `core/src/monitor.rs` Windows preemption (`preempt_thread` / `do_preempt` / `preempt_asm`) | Flaky: `preemptive.rs` `join.timeout_join(3s)` times out → `open-coroutine stop timeout` (30s) + `std::io::stdio RefCell already borrowed` panic on the hijacked thread. Reproduces locally ~1/3 in debug even single-threaded; release mostly passes; **fails on master CI** (`ci-preemptive.sh`) — pre-existing. | Root cause confirmed via backtrace: arbitrary-point `SuspendThread` hijack lands inside `println!` while it holds stdout's `RefCell`; the suspended coroutine keeps the borrow, the next coroutine's `println!` panics → `catch_unwind` → `CoroutineState::Error` → `TEST_FLAG1` never cleared → join/stop timeouts. `iocp,preemptive,ci` lacks `syscall` feature so `Sleep` is unhooked — stack hijack is the only scheduling mechanism. Fixes implemented on `iocp20261128` (not on master): `notify_queue` `UnsafeCell`→`Mutex` data race, `PREEMPT_PENDING` reset on cooperative yield, stack-remaining guard before forced suspend. **Residual (inherent)**: no safepoints in Rust — any non-reentrant state can be interrupted; same hazard class as Go async preemption. Options: test avoids `println!` in hot loop, accept documented flake, or safepoint mechanism (large). |
| B2 | medium | open | `core/src/net/operator/windows/mod.rs` `Operator::async_cancel` | Body is `todo!("CancelIoEx")` — panics if ever invoked, not a clean `Unsupported`. | Implement `CancelIoEx`-based cancel or return `ErrorKind::Unsupported` explicitly. Fix exists on `iocp20261128`. |
| B3 | medium | open | `core/src/net/operator/windows/mod.rs` `Operator::acceptex` | `while AcceptEx(...) == FALSE` retries forever on any error other than `WSA_IO_PENDING`; on hard failure the leaked `Overlapped` is never reclaimed. | Call once; break on `WSA_IO_PENDING`, reclaim + return on other errors. Live path — `accept`/`WSAAccept` are the only IOCP-routed syscalls on master. Fix exists on `iocp20261128`. |
| B4 | medium | open | `core/src/net/operator/windows/mod.rs` `Operator::acceptex` | Local-address `Vec<u8>` is dropped when `acceptex` returns while `AcceptEx` is still pending → completion writes into freed heap (UB). | Buffer must be owned by the leaked `Overlapped` (or otherwise kept alive until completion). Fix exists on `iocp20261128`. |
| B5 | medium | open | `core/src/net/event_loop.rs` `EventLoop::token` | Uses `GetCurrentThread()` — a pseudo-handle (`-2`) shared by all threads — as the per-thread token component, so concurrent IOCP operations from different threads collide on `syscall_wait_table` insert → panic. | Use `GetCurrentThreadId()`. Fix exists on `iocp20261128`. |
| B6 | high (latent) | open | `core/src/net/operator/windows/mod.rs` `Operator::wsarecv` / `Operator::wsasend` | They call `WSARecv`/`WSASend` resolved through normal imports — inside the hook DLL these are the **hooked** entry points → re-entrant syscall dispatch and state corruption. | Latent on master: no syscall routes recv/send through IOCP yet, so these methods are unreachable from the dispatch chain. **Must be fixed before wiring IOCP recv/send** — `iocp20261128` solves it via a `REAL_SYSCALLS` trampoline registry in `hook/src/syscall/windows.rs`. Note: `WSARecvFrom`/`WSASendTo` were tried as unhooked variants and abandoned — ws2_32 forwards internally back to the hooked entry points. |
| B7 | low | open | `core/src/syscall/windows/connect.rs` | `connect` timeout maps `WSAETIMEDOUT` back to `WSAEINPROGRESS` — pre-existing behavior. | Verify whether the mapping is intentional; if so document it in syscall docs. |
| B8 | low | open | `core/src/syscall/windows/` `select`/`WSAPoll` NIO emulation | Edge cases not exhaustively verified (nfds truncation, revents writeback). | Audit against real `select`/`WSAPoll` semantics; fix implementation, not tests. |
| B9 | info | open | `core/src/syscall/windows/` dispatch (`impl_syscall!`) | On master the IOCP backend is wired **only for `accept`/`WSAAccept`**; `recv`/`send`/`WSARecv`/`WSASend`/`connect`/etc. degrade to `Facade -> Nio -> Raw` even with `feature = "iocp"`. The `impl_iocp*` macros in `syscall/windows/mod.rs` are defined but unused. | Not a bug — an incomplete feature. `iocp20261128` wires the full socket data plane; it depends on B6 being fixed first. |

## Triage notes

- **B1 is the only entry currently blocking Windows CI** (`ci-preemptive.sh`
  fails on master). B3–B5 are live defects on the accept path but rarely
  triggered; B6 is a hard blocker for the IOCP data-plane branch.
- When fixing B1, the stdio `RefCell` panic is a *symptom* (hijacked thread
  interrupted mid-`println!`), not an independent bug in the preempt path's
  own logging — fix at the scheduling layer per the root-cause rule.
