# open-coroutine

## This crate is a try to use `corosensei` as low_level coroutine

## ~~Problem1~~
Can't use backtrace, you can see these in :

src/coroutine/mod.rs:287

src/scheduler.rs:182

Solved by increasing stack size.

## Problem2
Can't pass the linux preemptive schedule CI(but pass the macos CI), see it in :

examples/preemptive.rs

Note: `monitor` mod register the `signal handler`.
