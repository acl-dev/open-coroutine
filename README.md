# open-coroutine

## This crate is a try to use `corosensei` as low_level coroutine

## Problem1
Can't use backtrace, you can see these in :
src/coroutine/mod.rs:287
src/scheduler.rs:182

## Problem2
Can't pass the complex preemptive schedule CI, see it in :
src/scheduler.rs:228

Note: `monitor` mod register the `signal handler`.
