#!/bin/bash

cd open-coroutine-timer
cargo publish

cd ..
cd open-coroutine-queue
cargo publish

cd ..
cd open-coroutine-core
cargo publish

cd ..
cd open-coroutine-hooks
cargo publish

cd ..
cd open-coroutine-macros
cargo publish

cd ..
cd open-coroutine
cargo publish
