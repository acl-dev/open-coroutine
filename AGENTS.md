# AGENTS.md

Guidance for AI agents working in this repository.

## Toolchain & verification

- Rust **nightly** is required for development (`cargo +nightly ...`); the workspace uses unstable features.
- **Self-test before declaring work done.** Never hand off unverified changes;
  run the relevant tests yourself and report actual results.
- Standard local verification:
  - `cargo +nightly fmt --all -- --check`
  - `cargo +nightly clippy --all --all-features -- -D warnings` (CI gates on
    this — plain clippy without `-D warnings` misses what CI rejects)
  - `cargo +nightly test --all --all-features`
  - `cargo +nightly test -p open-coroutine-core --features iocp` (core unit tests)
- Feature flags: `io_uring` (Linux), `iocp` (Windows), `completion_io` (both), `ci` (CI test mode), `hook`, `net`, `preemptive`, `log`, `syscall`.

### CI test matrix (`.github/workflows/ci.sh`)

CI does **not** use `--all-features`. Per target it runs, in both debug and
`--release`, with `RUST_TEST_THREADS=1` and `RUST_BACKTRACE=1`:

- `core` and `open-coroutine`: `--features ci`
- Linux `x86_64-unknown-linux-gnu` additionally: `--no-default-features --features io_uring,ci`
- Windows additionally: `--no-default-features --features iocp,ci`

When touching platform backends, replicate the matching row locally — e.g. on
Windows run `cargo +nightly test --no-default-features --features iocp,ci`
for both `core` and `open-coroutine` packages, debug and release.

Locally, either export `RUST_TEST_THREADS=1` or pass `-- --test-threads=1`.
Do **not** run the test suite in parallel: `CoroutinePool` instances share a
process-global task queue (`TASK_GLOBAL_QUEUE_BEAN`) plus global
`RUNNING_TASKS`/`CANCEL_TASKS` statics, so `core/tests/co_pool.rs` tests
interfere with each other and flake (e.g. `pool.is_empty()` assertion).
This is a test-isolation limitation, not a `try_cancel_task` race — the
cancel path itself uses `DashMap`/`DashSet` and is thread-safe.

## Architecture facts that will bite you

### The hook DLL embeds its own copy of open-coroutine-core

`open-coroutine-hook` is a `cdylib` that **statically links its own copy of
`open-coroutine-core`**. Intercepted syscalls execute inside the DLL's core
instance (its own scheduler TLS, EventLoops, IOCP operator, wait tables), not
the executable's. Consequences:

- Thread-local state is **not shared** between the exe's core and the DLL's
  core. Do not design cross-boundary TLS flags (a bypass flag set by the exe
  is invisible inside the DLL).
- `open-coroutine/build.rs` shells out to `cargo build` for the hook and
  copies `open_coroutine_hook.dll` into the target profile/deps dirs. It
  declares `cargo:rerun-if-changed` for `../hook/src` and `../core/src` in the
  workspace layout, so editing those sources retriggers the DLL rebuild.
  **If tests behave as if recent core/hook changes had no effect, the DLL is
  stale — force a rebuild** (e.g. `touch open-coroutine/build.rs` or delete
  `target/debug/{,deps/}open_coroutine_hook.dll` and rebuild).

### Syscall dispatch chain (Windows)

`Facade -> Iocp -> Nio -> Raw` under `feature = "iocp"`:

- `Facade` (`impl_facade!`): coroutine state transitions + enter/exit logging.
- `Iocp` (`impl_iocp*` macros / hand-written impls in `core/src/syscall/windows/`):
  submits overlapped I/O via `crate::net::EventLoops` -> `EventLoop` ->
  `net::operator::windows::Operator`, suspends the coroutine, waits on the
  completion port.
- `Nio`: non-blocking emulation fallback (set_non_blocking + event wait).
- `Raw`: calls the real function via `fn_ptr` (the minhook trampoline).

### Operator must not call hooked Winsock APIs

The hook patches `WSARecv`, `WSASend`, `WSASocketW`, `setsockopt`, `accept`,
`WSAAccept`, `connect`, `ioctlsocket`, `listen`, `recv`, `send`, `shutdown`,
`socket`, `select`, `WSAPoll`, `Sleep`, `CreateFileW`, `SetFilePointerEx`
(see `hook/src/syscall/windows.rs`). Calling these from inside the operator
re-enters the syscall layer and corrupts state / can null-deref.

- Overlapped recv/send use **`WSARecvFrom`/`WSASendTo`** with NULL address
  arguments — they are NOT hooked and are signature-compatible.
- `AcceptEx`, `getsockopt`, `CreateIoCompletionPort`, `GetQueuedCompletionStatusEx`
  are not hooked and safe to call directly.
- `WSASocketW`/`setsockopt` calls inside the operator still go through the
  hook but are synchronous and harmless (they re-enter the chain and return).

### Overlapped socket tracking

`core/src/net/operator/windows/mod.rs` keeps `OVERLAPPED_SOCKETS` (sockets
created with `WSA_FLAG_OVERLAPPED` or accepted via `AcceptEx`) and
`BOUND_SOCKETS` (sockets already bound to our IOCP port — a socket can only
bind to one completion port). Overlapped ops on untracked sockets are
rejected so the caller falls back to NIO.

### Return-value semantics must match the real syscall

The hook must NOT change syscall semantics. `WSARecv`/`WSASend` return 0 on
success and write the byte count to `lpnumberofbytesrecvd`/
`lpnumberofbytessent`; `recv`/`send` return the byte count; `accept` returns
the new socket or `INVALID_SOCKET`. If a test seems to require different
semantics, fix the implementation — do not bend tests to fit.

When a syscall's success return differs from "byte count", write a
hand-rolled `Iocp*` impl (see `accept.rs`, `WSARecv.rs`, `WSASend.rs`) instead
of reusing `impl_iocp_read!`/`impl_iocp_write!`, which always return the
transferred byte count.

## Pitfalls already hit (do not repeat)

- **`GetCurrentThread()` is a pseudo-handle** shared by all threads — using
  it for per-thread tokens makes every thread collide on the same
  `syscall_wait_table` key and panics on insert. Use
  `GetCurrentThreadId()` (`core/src/net/event_loop.rs` `token()`).
- **Overlapped I/O lifetimes**: `WSABUF` arrays, `lpflags`, and AcceptEx
  address buffers must outlive the async operation. They live inside the
  leaked `Overlapped` struct (`wsabuf`, `flags`, `accept_buf` fields) and are
  reclaimed via `Box::from_raw` on IOCP completion — never point WSA APIs at
  stack/temporary storage.
- **`AcceptEx` error loop**: a `while AcceptEx(...) == FALSE` retry loop
  spins forever on non-`WSA_IO_PENDING` errors. Call once; on error reclaim
  the leaked `Overlapped` and return.
- **`Vec::from_raw_parts` on caller WSABUF arrays** (NIO iovec path) must be
  paired with `mem::forget` — it takes ownership of memory it does not own.
- **Git Bash `> nul` creates a real file** named `nul` in the repo root
  (Windows `nul` device semantics only apply in cmd). Use `> /dev/null`.
- **Do not assume ws2_32 forwards between entry points**: `WSARecvFrom` /
  `WSASendTo` turned out NOT to re-enter the hooked `WSARecv`/`WSASend` —
  but verify empirically before relying on any "unhooked variant" trick.
- **`error!`/`info!`/`warn!` expand to nothing without the `log` feature** —
  bindings used only inside them become `unused_variables` under
  `deny(warnings)` in `--no-default-features` builds. Prefix with `_` and pass
  positionally (e.g. `error!("... {}", _other)`).
- **TLS flags cannot cross the exe/DLL boundary** (see architecture facts) —
  a thread-local "inside operator" marker set by the exe's core is invisible
  to the hook DLL's core copy.

## Known gaps / quirks

- `async_cancel` on IOCP returns `Unsupported`.
- `connect` maps `WSAETIMEDOUT` back to `WSAEINPROGRESS` on timeout
  (`core/src/syscall/windows/connect.rs`).
- Caller-supplied `lpoverlapped`/`lpcompletionroutine` on `WSARecv`/`WSASend`
  falls back to the real async path via NIO.
- `select`/`WSAPoll` NIO emulation edge cases are not exhaustively verified.

## Git conventions

- Commit messages: single concise line matching repo style; **no AI/agent
  attribution or trailers**.
