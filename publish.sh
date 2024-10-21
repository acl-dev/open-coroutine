#!/bin/bash

cd core
cargo publish

cd ../hook
cargo publish

cd ../macros
cargo publish

cd ../open-coroutine
cargo publish
