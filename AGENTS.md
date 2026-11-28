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
- Feature flags: `io_uring` (Linux), `iocp` (Windows), `completion_io` (both), `ci` (CI test mode), `net`, `preemptive`, `log`, `syscall`, `korosensei` (internal).

### CI test matrix (`.github/workflows/ci.sh`, `ci-preemptive.sh`)

CI does **not** use `--all-features`. Per target it runs, in both debug and
`--release`, with `RUST_TEST_THREADS=1` and `RUST_BACKTRACE=1`:

- `core` and `open-coroutine`: `--features ci`
- Linux `x86_64-unknown-linux-gnu` additionally: `--no-default-features --features io_uring,ci`
- Windows additionally: `--no-default-features --features iocp,ci`

`ci-preemptive.sh` repeats the same matrix with `preemptive` added to the
feature set (e.g. `--no-default-features --features iocp,preemptive,ci` on
Windows). This is the step that currently fails on master — see BUGS.md B1.

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

## Working rules (ported from ModularComponents6 conventions)

These are behavioral rules, not project facts. They apply to every change.

### Ask before guessing

If anything is uncertain — an API's contract, a feature interaction, a design
trade-off that affects safety/compatibility — stop and ask. Do not write code
on "probably". When asking, state: the uncertain point, your preferred option,
the alternative, and the impact of each. Read-only investigation and anything
verifiable from the source/docs does not need approval.

### Bugs outrank features

- Report a bug the moment you find one — do not batch it into a final summary.
- Fix at the root cause. No exit-point patches: no added null-checks, retries,
  swallowed errors, or special cases that hide the symptom. Check sibling call
  paths of the same root cause too.
- Test-first for bug fixes and behavior changes: write a failing test that
  reproduces the root cause, confirm it is red for the right reason, then fix
  and watch it go green. Do not fix first and add tests afterwards.
- Report locations as `Type::method` / `module::function` anchors — **never
  bare line numbers** (they rot under parallel edits). File links are fine as
  supplementary pointers.
- One bug = one commit/PR. Do not mix an unrelated fix into the same change.

### One thing at a time

A round of work does one thing: one bug fix, one feature, one refactor. You
may *propose* a batch ("these share a root cause, suggest fixing together")
but must not execute it without explicit approval. Group candidates before
acting: (1) changes observable behavior, (2) robustness/defensive only,
(3) diagnostics/logging — only group 1 may proceed unprompted.

"One thing" is defined by **acceptance criteria and dependency**, not by
file boundaries. A fix that is a correctness prerequisite of the feature
belongs in the same change — e.g. operator lifetime/loop fixes were required
for the IOCP wiring to be *correct*, not merely to pass tests ("it happens
to run" ≠ "it is handled"). Conversely, never pad a change with unrelated
work that shares only a file or a session.

### Comments & TODOs

- Comments explain *why* / invariants / ordering constraints, not what the
  code does. If deleting a comment loses nothing, delete it.
- `TODO`s must carry scope and an exit condition (`TODO(iocp): ...`); bare
  `// todo` / `// fix later` without an owner path are not allowed.
- Files past ~800 lines deserve a layering review note to the user — a
  reminder, not an automatic split.

### Document philosophy

Docs in this repo are **falsifiable claims, not answers**. When a doc
contradicts the code, report the contradiction — do not edit code to match
the doc, and do not cite the doc as evidence without `file::symbol` proof.
Ledger-style docs (bug lists, audit trails) are not preloaded; open them
only when the task points there.

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

`impl_syscall!` in `core/src/syscall/windows/mod.rs` comes in two forms:

- 3-arg: `Facade -> Nio -> Raw`
- 4-arg: `Facade -> Iocp -> Nio -> Raw` (only compiled under
  `all(windows, feature = "iocp")`)

On master **only `accept.rs` uses the Iocp slot** (`IocpAcceptSyscall` →
`net::EventLoops::accept` → `EventLoop` → `net::operator::windows::Operator::acceptex`).
Every other Windows syscall — `recv`, `send`, `WSARecv`, `WSASend`, `connect`,
`select`, `WSAPoll`, file IO, etc. — is `Facade -> Nio -> Raw` even with
`feature = "iocp"`. The `impl_iocp!`/`impl_iocp_read!`/`impl_iocp_write!`
macros exist but are currently unused (BUGS.md B9).

- `Facade` (`impl_facade!`): coroutine state transitions + enter/exit logging.
- `Iocp`: overlapped submission via the completion port, coroutine suspend,
  wait for completion.
- `Nio`: non-blocking emulation fallback (set_non_blocking + event wait).
- `Raw`: calls the real function via `fn_ptr` (the minhook trampoline).

### Operator and hooked Winsock APIs — known hazard

The hook patches `WSARecv`, `WSASend`, `WSASocketW`, `setsockopt`, `accept`,
`WSAAccept`, `connect`, `ioctlsocket`, `listen`, `recv`, `send`, `shutdown`,
`socket`, `select`, `WSAPoll`, `Sleep`, `CreateFileW`, `SetFilePointerEx`
(see `hook/src/syscall/windows.rs`). Inside the hook DLL, calls to these
names resolve to the **hooked** entry points.

`Operator::wsarecv`/`Operator::wsasend` currently call `WSARecv`/`WSASend`
through plain imports — i.e. the hooked functions — which re-enters the
syscall layer and corrupts state. This is latent today only because nothing
routes recv/send through IOCP (BUGS.md B6); it must be fixed before the data
plane is wired. `AcceptEx`, `getsockopt`, `CreateIoCompletionPort`,
`GetQueuedCompletionStatusEx` are not hooked and safe to call directly.

### Known IOCP operator defects (master)

All live on the accept path unless noted — see BUGS.md for status:

- `Operator::acceptex`: `while AcceptEx(...) == FALSE` spins forever on
  non-`WSA_IO_PENDING` errors and leaks the `Overlapped` (B3).
- `Operator::acceptex`: the local-address `Vec<u8>` dies when the function
  returns while `AcceptEx` is pending — completion writes into freed memory
  (B4).
- `Operator::async_cancel` is `todo!("CancelIoEx")` (B2).
- `EventLoop::token` uses the `GetCurrentThread()` pseudo-handle, so
  concurrent IOCP ops from different threads collide (B5).
- There is no overlapped-socket tracking (`OVERLAPPED_SOCKETS`/
  `BOUND_SOCKETS` do not exist on master); the data-plane branch adds it.

### Return-value semantics must match the real syscall

The hook must NOT change syscall semantics. `accept`/`WSAAccept` return the
new socket or `INVALID_SOCKET`; `recv`/`send` return the byte count;
`WSARecv`/`WSASend` return 0 on success and write the byte count to
`lpnumberofbytesrecvd`/`lpnumberofbytessent`. If a test seems to require
different semantics, fix the implementation — do not bend tests to fit.

On master the only hand-written `Iocp*` impl is `IocpAcceptSyscall` in
`accept.rs` — its success return is a socket handle, not a byte count, which
is why it cannot use the `impl_iocp_read!`/`impl_iocp_write!` macros (they
always return the transferred count). Any future `Iocp*` impl for a syscall
whose success return is not a byte count needs the same hand-written pattern.

## Pitfalls already hit (do not repeat)

Live IOCP operator bugs are tracked in [BUGS.md](BUGS.md) (B2–B6) — do not
re-derive them. General pitfalls:

- **`Vec::from_raw_parts` on caller WSABUF arrays** (NIO iovec path) must be
  paired with `mem::forget` — it takes ownership of memory it does not own.
- **Git Bash `> nul` creates a real file** named `nul` in the repo root
  (Windows `nul` device semantics only apply in cmd). Use `> /dev/null`.
- **Do not assume ws2_32 forwards between entry points**: `WSARecvFrom` /
  `WSASendTo` were tried as unhooked variants for operator overlapped I/O and
  abandoned — ws2_32 forwards internally back to the hooked `WSARecv`/
  `WSASend`. Verify any "unhooked variant" trick empirically.
- **`error!`/`info!`/`warn!` expand to nothing without the `log` feature** —
  bindings used only inside them become `unused_variables` under
  `deny(warnings)` in `--no-default-features` builds. `_`-prefixing is not
  the fix: pedantic clippy rejects `used_underscore_binding` when `log` IS
  enabled. Keep the plain name and put `#[allow(unused_variables)]` on the
  match arm / statement instead.
- **TLS flags cannot cross the exe/DLL boundary** (see architecture facts) —
  a thread-local "inside operator" marker set by the exe's core is invisible
  to the hook DLL's core copy.

## Known gaps / quirks

The authoritative ledger is [BUGS.md](BUGS.md) — check it before starting
work in an area and update it in the same commit as any fix. Headline items
on master:

- Windows preemptive mode is flaky (arbitrary-point stack hijack vs
  non-reentrant stdio; fails on master CI) — BUGS.md B1.
- IOCP backend covers `accept`/`WSAAccept` only; the socket data plane is
  NIO — BUGS.md B9, blocked by B6.
- `Operator::acceptex` has a live infinite-retry loop and a pending-buffer
  UAF — BUGS.md B3/B4.
- `connect` maps `WSAETIMEDOUT` back to `WSAEINPROGRESS` on timeout
  (`core/src/syscall/windows/connect.rs`, BUGS.md B7).
- `select`/`WSAPoll` NIO emulation edge cases are not exhaustively verified
  (BUGS.md B8).

## Git conventions

- Commit messages: single concise line matching repo style; **no AI/agent
  attribution or trailers**.
