#!/bin/bash

cd core
cargo publish --registry crates-io

cd ../hook
cargo publish --registry crates-io

cd ../macros
cargo publish --registry crates-io

cd ../open-coroutine
cargo publish --registry crates-io
