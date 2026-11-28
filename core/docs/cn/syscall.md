---
title: 系统调用分发
date: 2026-11-28 00:00:00
author: loongs-zhang
---

# 系统调用分发

[English](../en/syscall.md) | 中文

## 概述

`open-coroutine-hook` 使用 [minhook](https://github.com/TsudaKageyu/minhook) 拦截一组固定的慢系统调用（socket IO、sleep、文件 IO，
见 [unix](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/unix.rs)/
[windows](https://github.com/acl-dev/open-coroutine/blob/master/hook/src/syscall/windows.rs)）。每个被拦截的调用都会转发到
`open_coroutine_core::syscall::<name>(fn_ptr, args...)`，其中`fn_ptr`是指向原始函数的trampoline。

> **注意：** hook cdylib静态链接了自己的一份`open-coroutine-core`，所以下文的分发链运行在DLL内嵌core实例中——线程本地状态与可执行文件的core不共享。

## 分发链

每个被hook的系统调用都经过四层链分发（平台后端禁用时为三层）：

```text
Facade -> Iocp -> Nio -> Raw        (windows + iocp)
Facade -> Nio   -> Raw              (其他情况)
```

| 层      | 职责 |
| ------- | ---- |
| Facade  | 协程状态机流转（`Running -> Syscall(Executing)`）、进入/退出日志、errno保存与恢复 |
| Iocp    | 通过`net::EventLoops` -> `EventLoop` -> `net::operator`提交overlapped IO，挂起协程，等待IOCP完成事件 |
| Nio     | 非阻塞模拟：`set_non_blocking` + 事件循环等待 + 重试循环 |
| Raw     | 通过`fn_ptr`（minhook trampoline）调用真实函数；未加载hook时走FFI导入 |

`Iocp`层仅在Windows且`feature = "iocp"`时存在，且在master上目前**只接了`accept`/`WSAAccept`**（`accept.rs`中的`IocpAcceptSyscall` → `net::EventLoops::accept` → `Operator::acceptex`）。其余所有Windows系统调用即使开启`iocp`也走`Facade -> Nio -> Raw`；`impl_iocp*`宏已定义但未被使用。Linux上对应的是`io_uring`（`feature = "io_uring"`）；其他平台链路退化为`Facade -> Nio -> Raw`。

## 平台后端

- **Linux `io_uring`**：文件与socket系统调用以SQE形式提交，完成事件从CQ ring收割。
- **Windows `IOCP`**：目前只有`accept`/`WSAAccept`以overlapped操作（`AcceptEx`）提交，绑定到每个事件循环的完成端口。尚无overlapped socket跟踪机制，且`Operator::wsarecv`/`wsasend`仍调用被hook的`WSARecv`/`WSASend`入口——这是已知隐患，必须先解决（如真实函数指针注册表）才能接通recv/send数据面。见[BUGS.md](../../../BUGS.md) B3–B6、B9。
- **NIO回退**：没有`Iocp`实现的所有系统调用、完成后端不可用、或调用方自带`lpoverlapped`/`lpcompletionroutine`时使用。

## 返回值语义

hook不得改变系统调用语义：

- `recv`/`send`返回传输字节数；`WSARecv`/`WSASend`成功返回0并把字节数写入`lpnumberofbytesrecvd`/`lpnumberofbytessent`；`accept`/`WSAAccept`返回新socket或`INVALID_SOCKET`。
- 错误返回平台错误哨兵值（`-1`/`SOCKET_ERROR`/`INVALID_SOCKET`）并设置errno。
- overlapped提交对调用方透明：不返回`SOCKET_ERROR + WSA_IO_PENDING`，而是挂起协程并在最终完成后返回结果——与io_uring后端提供的契约一致。

成功返回值不是字节数的系统调用需要手写`Iocp*`实现，而不是`impl_iocp_read!`/`impl_iocp_write!`宏——这些宏总是返回传输字节数。master上唯一的此类实现是`accept.rs`中的`IocpAcceptSyscall`（返回socket句柄）；`WSARecv`/`WSASend`尚无`Iocp`实现。

## 已hook的系统调用（Windows）

`Sleep`、`CreateFileW`、`SetFilePointerEx`、`accept`、`WSAAccept`、`connect`、`ioctlsocket`、`listen`、`recv`、
`send`、`shutdown`、`socket`、`setsockopt`、`WSARecv`、`WSASend`、`WSASocketW`、`select`、`WSAPoll`。

## 已知限制

- IOCP覆盖范围仅限`accept`/`WSAAccept`；即使开启`feature = "iocp"`，其余socket与文件系统调用也走NIO后端。
- `Operator::async_cancel`是`todo!("CancelIoEx")`——被调用会panic。
- `connect`超时把`WSAETIMEDOUT`映射回`WSAEINPROGRESS`。
- `Operator::acceptex`存在对非`WSA_IO_PENDING`错误的死循环重试和挂起缓冲区生命周期bug——见[BUGS.md](../../../BUGS.md) B3/B4。
