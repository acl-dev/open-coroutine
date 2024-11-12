#!/usr/bin/env sh

set -ex

CARGO=cargo
if [ "${CROSS}" = "1" ]; then
    export CARGO_NET_RETRY=5
    export CARGO_NET_TIMEOUT=10

    cargo install cross
    CARGO=cross
fi

# If a test crashes, we want to know which one it was.
export RUST_TEST_THREADS=1
export RUST_BACKTRACE=1

# test open-coroutine-core mod
cd "${PROJECT_DIR}"/core
"${CARGO}" test --target "${TARGET}"
"${CARGO}" test --target "${TARGET}" --release

# test open-coroutine
cd "${PROJECT_DIR}"/open-coroutine
"${CARGO}" test --target "${TARGET}"
"${CARGO}" test --target "${TARGET}" --release

# test io_uring
if [ "${TARGET}" = "x86_64-unknown-linux-gnu" ]; then
    cd "${PROJECT_DIR}"/core
    "${CARGO}" test --target "${TARGET}" --no-default-features --features io_uring
    "${CARGO}" test --target "${TARGET}" --no-default-features --features io_uring --release
    cd "${PROJECT_DIR}"/open-coroutine
    "${CARGO}" test --target "${TARGET}" --no-default-features --features io_uring
    "${CARGO}" test --target "${TARGET}" --no-default-features --features io_uring --release
fi

# test IOCP
if [ "${OS}" = "windows-latest" ]; then
    cd "${PROJECT_DIR}"/core
    "${CARGO}" test --target "${TARGET}" --no-default-features --features iocp
    "${CARGO}" test --target "${TARGET}" --no-default-features --features iocp --release
    cd "${PROJECT_DIR}"/open-coroutine
    "${CARGO}" test --target "${TARGET}" --no-default-features --features iocp
    "${CARGO}" test --target "${TARGET}" --no-default-features --features iocp --release
fi
