# open-coroutine

[![crates.io](https://img.shields.io/crates/v/open-coroutine.svg)](https://crates.io/crates/open-coroutine)
[![docs.rs](https://img.shields.io/badge/docs-release-blue)](https://docs.rs/open-coroutine)
[![LICENSE](https://img.shields.io/github/license/acl-dev/open-coroutine.svg?style=flat-square)](https://github.com/acl-dev/open-coroutine/blob/master/LICENSE-APACHE)
[![Build Status](https://github.com/acl-dev/open-coroutine/workflows/CI/badge.svg)](https://github.com/acl-dev/open-coroutine/actions)
[![Codecov](https://codecov.io/github/acl-dev/open-coroutine/graph/badge.svg?token=MSM3R7CBEX)](https://codecov.io/github/acl-dev/open-coroutine)
[![Average time to resolve an issue](http://isitmaintained.com/badge/resolution/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "解决issue的平均时间")
[![Percentage of issues still open](http://isitmaintained.com/badge/open/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "仍未关闭issue的百分比")

`open-coroutine`是一个简单、高效、通用的有栈协程库。

[English](README.md) | 中文

## 🚀 当前特性

- [x] 抢占调度(`不支持windows`): 即使协程进入死循环，它仍能被抢占，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/preemptive.rs);
- [x] Hook: 您可以在协程中自由使用大多数慢系统调用，查看支持的系统调用[unix](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/unix.rs)/[windows](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/windows.rs);
- [x] 可伸缩栈: 协程栈的大小支持无限制扩容而没有复制堆栈的开销，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/scalable_stack.rs);
- [x] io_uring(`只支持linux`): 在本地文件IO和网络IO方面支持并兼容io_uring。如果您的系统不支持，它将回退到NIO;
- [x] 优先级: 支持自定义任务优先级，注意协程优先级未对用户开放;
- [x] 任务窃取: 内部使用无锁任务窃取队列;
- [x] 兼容性: open-coroutine的实现是No async的，但它与async兼容，这意味着您可以在`tokio/sync-std/smol/...`中使用这个crate;
- [x] 跨平台: 支持Linux、macOS和Windows;

## 🕊 未来计划

- [ ] 取消协程/任务;
- [ ] 增加性能指标;
- [ ] 增加并发工具包;
- [ ] 支持AF_XDP套接字;

## 📖 快速接入

### step1: 在你的Cargo.toml中添加依赖

```toml
[dependencies]
# check https://crates.io/crates/open-coroutine
open-coroutine = "x.y.z"
```

### step2: 添加`open_coroutine::main`宏

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
    _ = open_coroutine::task!(|param| {
        assert_eq!(param, "param");
    }, "param");
}
```

### 创建具有优先级的任务(可选)

```rust
#[open_coroutine::main]
fn main() {
    _ = open_coroutine::task!(|param| {
        assert_eq!(param, "param");
    }, "param", 1/*数值越小，优先级越高*/);
}
```

### 等待任务完成或超时(可选)

```rust
#[open_coroutine::main]
fn main() {
    let task = open_coroutine::task!(|param| {
        assert_eq!(param, "param");
    }, "param", 1);
    task.timeout_join(std::time::Duration::from_secs(1)).expect("timeout");
}
```

### 扩容栈(可选)

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

## ⚓ 了解更多

- [诞生之因](docs/cn/background.md)
- [语言选择](docs/cn/why-rust.md)

[我有故事,你有酒吗?](https://github.com/acl-dev/open-coroutine-docs)
