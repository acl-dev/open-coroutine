# open-coroutine

[![crates.io](https://img.shields.io/crates/v/open-coroutine.svg)](https://crates.io/crates/open-coroutine)
[![docs.rs](https://img.shields.io/badge/docs-release-blue)](https://docs.rs/open-coroutine)
[![LICENSE](https://img.shields.io/github/license/acl-dev/open-coroutine.svg?style=flat-square)](https://github.com/acl-dev/open-coroutine/blob/master/LICENSE-APACHE)
[![Build Status](https://github.com/acl-dev/open-coroutine/workflows/CI/badge.svg)](https://github.com/acl-dev/open-coroutine/actions)
[![Codecov](https://codecov.io/github/acl-dev/open-coroutine/graph/badge.svg?token=MSM3R7CBEX)](https://codecov.io/github/acl-dev/open-coroutine)
[![Average time to resolve an issue](http://isitmaintained.com/badge/resolution/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "解决issue的平均时间")
[![Percentage of issues still open](http://isitmaintained.com/badge/open/acl-dev/open-coroutine.svg)](http://isitmaintained.com/project/acl-dev/open-coroutine "仍未关闭issue的百分比")

`open-coroutine`是一个简单、高效、通用的有栈协程库，您可以将其用作IO线程池的性能替代，查看[为什么更好](core/docs/cn/why-better.md).

[English](README.md) | 中文

## 🚀 当前特性

- [x] 抢占调度:
  即使协程进入死循环，它仍能被抢占，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/preemptive.rs);
- [x] Hook:
  您可以在协程中自由使用大多数慢系统调用，查看支持的系统调用[unix](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/unix.rs)/[windows](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/windows.rs);
- [x] 可伸缩栈:
  协程栈的大小支持无限制扩容而没有复制堆栈的开销，查看[例子](https://github.com/loongs-zhang/open-coroutine/blob/master/open-coroutine/examples/scalable_stack.rs);
- [x] io_uring(`只支持linux`): 在本地文件IO和网络IO方面支持并兼容io_uring。如果您的系统不支持，它将回退到NIO;
- [x] 优先级: 支持自定义任务优先级，注意协程优先级未对用户开放;
- [x] 任务窃取: 内部使用无锁任务窃取队列;
- [x] 兼容性: open-coroutine的实现是No async的，但它与async兼容，这意味着您可以在`tokio/sync-std/smol/...`中使用这个crate;
- [x] 跨平台: 支持Linux、macOS和Windows;

## 🕊 未来计划

- [ ] 
  增加性能[基准测试](https://github.com/TechEmpower/FrameworkBenchmarks/wiki/Project-Information-Framework-Tests-Overview);
- [ ] 增加性能指标监控;
- [ ] 增加并发工具包;
- [ ] 支持AF_XDP套接字;

## 🏠 架构设计

```mermaid
graph TD
    subgraph ApplicationFramework
        Tower
        Actix-Web
        Rocket
        warp
        axum
    end
    subgraph MessageQueue
        RocketMQ
        Pulsar
    end
    subgraph RemoteProcedureCall
        Dubbo
        Tonic
        gRPC-rs
        Volo
    end
    subgraph Database
        MySQL
        Oracle
    end
    subgraph NetworkFramework
        Tokio
        monoio
        async-std
        smol
    end
    subgraph open-coroutine-architecture
        subgraph core
            Preemptive
            ScalableStack
            WorkSteal
            Priority
        end
        subgraph hook
            HookSyscall
        end
        subgraph macros
            open-coroutine::main
        end
        subgraph open-coroutine
        end
        hook -->|depends on| core
        open-coroutine -->|link| hook
        open-coroutine -->|depends on| macros
    end
    subgraph OperationSystem
        Linux
        macOS
        Windows
    end
    ApplicationFramework -->|maybe depends on| RemoteProcedureCall
    ApplicationFramework -->|maybe depends on| MessageQueue
    ApplicationFramework -->|maybe depends on| Database
    MessageQueue -->|depends on| NetworkFramework
    RemoteProcedureCall -->|depends on| NetworkFramework
    NetworkFramework -->|runs on| OperationSystem
    NetworkFramework -->|can depends on| open-coroutine-architecture
    Database -->|runs on| OperationSystem
    open-coroutine-architecture -->|runs on| OperationSystem
```

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

## 🪶 进阶使用

### 创建具有优先级的任务

```rust
#[open_coroutine::main]
fn main() {
    _ = open_coroutine::task!(|param| {
        assert_eq!(param, "param");
    }, "param", 1/*数值越小，优先级越高*/);
}
```

### 等待任务完成或超时

```rust
#[open_coroutine::main]
fn main() {
    let task = open_coroutine::task!(|param| {
        assert_eq!(param, "param");
    }, "param", 1);
    task.timeout_join(std::time::Duration::from_secs(1)).expect("timeout");
}
```

### 等待任一任务完成或超时

```rust
#[open_coroutine::main]
fn main() {
    let result = open_coroutine::any_timeout_join!(
        std::time::Duration::from_secs(1),
        open_coroutine::task!(|_| 1, ()),
        open_coroutine::task!(|_| 2, ()),
        open_coroutine::task!(|_| 3, ())
    ).expect("timeout");
}
```

### 可伸缩栈

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

- [项目总览](core/docs/cn/overview.md)
- [诞生之因](docs/cn/background.md)
- [语言选择](docs/cn/why-rust.md)
- [协程总览](core/docs/cn/coroutine.md)
- [可伸缩栈总览](core/docs/cn/scalable-stack.md)
- [Monitor总览](core/docs/cn/monitor.md)
- [工作窃取总览](core/docs/cn/work-steal.md)
- [有序工作窃取总览](core/docs/cn/ordered-work-steal.md)
- [协程池总览](core/docs/cn/coroutine-pool.md)
- [Hook总览](hook/docs/cn/hook.md)

[旧版文档在这](https://github.com/acl-dev/open-coroutine-docs)

## 👍 鸣谢

这个crate的灵感来自以下项目：

- [acl](https://github.com/acl-dev/acl)
- [coost](https://github.com/idealvin/coost)
- [golang](https://github.com/golang/go)
- [stacker](https://github.com/rust-lang/stacker)
- [monoio](https://github.com/bytedance/monoio)
- [compio](https://github.com/compio-rs/compio)
- [may](https://github.com/Xudong-Huang/may)

感谢那些提供帮助的人：

[![Amanieu](https://images.weserv.nl/?url=avatars.githubusercontent.com/Amanieu?v=4&h=79&w=79&fit=cover&mask=circle&maxage=7d)](https://github.com/Amanieu)
[![bjorn3](https://images.weserv.nl/?url=avatars.githubusercontent.com/bjorn3?v=4&h=79&w=79&fit=cover&mask=circle&maxage=7d)](https://github.com/bjorn3)
[![workingjubilee](https://images.weserv.nl/?url=avatars.githubusercontent.com/workingjubilee?v=4&h=79&w=79&fit=cover&mask=circle&maxage=7d)](https://github.com/workingjubilee)
[![Noratrieb](https://images.weserv.nl/?url=avatars.githubusercontent.com/Noratrieb?v=4&h=79&w=79&fit=cover&mask=circle&maxage=7d)](https://github.com/Noratrieb)
