# open-coroutine

[![crates.io](https://img.shields.io/crates/v/open-coroutine.svg)](https://crates.io/crates/open-coroutine)
[![docs.rs](https://img.shields.io/badge/docs-release-blue)](https://docs.rs/open-coroutine)
[![LICENSE](https://img.shields.io/github/license/acl-dev/open-coroutine.svg?style=flat-square)](https://github.com/acl-dev/open-coroutine/blob/master/LICENSE-APACHE)
[![Build Status](https://github.com/acl-dev/open-coroutine/workflows/CI/badge.svg)](https://github.com/acl-dev/open-coroutine/actions)
[![Codecov](https://codecov.io/github/acl-dev/open-coroutine/graph/badge.svg?token=MSM3R7CBEX)](https://codecov.io/github/acl-dev/open-coroutine)
[![Average time to resolve an issue](http://isitmaintained.com/badge/resolution/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "Average time to resolve an issue")
[![Percentage of issues still open](http://isitmaintained.com/badge/open/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "Percentage of issues still open")

`open-coroutine`是一个简单、高效、通用的有栈协程库。

[English](README.md) | 中文

## 🚀 当前特性

- [x] 抢占调度(`不支持windows`): 即使协程进入死循环，它仍能被抢占，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/preemptive.rs);
- [x] Hook: 您可以在协程中自由使用大多数慢系统调用;
- [x] 可伸缩栈: 协程栈的大小支持无限制扩容而没有复制堆栈的开销，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/scalable_stack.rs);
- [x] io_uring(`只支持linux`): 在本地文件IO和网络IO方面支持并兼容io_uring。如果您的系统不支持，它将回退到NIO;
- [x] 优先级: 支持自定义任务和协程的优先级;
- [x] 任务窃取: 内部使用无锁任务窃取队列;
- [x] 兼容性: open-coroutine的实现是No async的，但它与async兼容，这意味着您可以在tokio/sync-std/smol/...中使用这个crate;
- [x] 跨平台: 支持Linux、macOS和Windows;

## 🕊 未来计划

- [ ] 支持`#[open_coroutine::all_join]`和`#[open_coroutine::any_join]`宏;
- [ ] 增加并发工具包;
- [ ] 支持AF_XDP套接字;

## 📖 快速接入

### step1: 在你的Cargo.toml中添加依赖

```toml
[dependencies]
# check https://crates.io/crates/open-coroutine
open-coroutine = "x.y.z"
```

### step2: 添加宏

```rust
#[open_coroutine::main]
fn main() {
    //......
}
```

### step3: 创建任务

```rust
#[open_coroutine::main]
fn main() {
    let task = open_coroutine::task!(|param| {
        assert_eq!(param, 1);
    }, 1);
    task.timeout_join(std::time::Duration::from_secs(1)).expect("timeout");
}
```

### step4: 扩容栈(可选)

```rust
#[open_coroutine::main]
fn main() {
    _ = open_coroutine::task!(|_| {
        fn recurse(i: u32, p: &mut [u8; 10240]) {
            open_coroutine::maybe_grow!(|| {
                // Ensure the stack allocation isn't optimized away.
                unsafe { _ = std::ptr::read_volatile(&p) };
                if i > 0 {
                    recurse(i - 1, &mut [0; 10240]);
                }
            })
            .expect("allocate stack failed")
        }
        println!("[task] launched");
        // Use ~500KB of stack.
        recurse(50, &mut [0; 10240]);
    }, ());
}
```

## ⚓ 学习更多

[我有故事,你有酒吗?](https://github.com/acl-dev/open-coroutine-docs)
