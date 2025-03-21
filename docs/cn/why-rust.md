---
title: 语言选择
date: 2024-12-29 10:00:00
author: loongs-zhang
---

# 语言选择

[English](../en/why-rust.md) | 中文

开发open-coroutine用什么语言呢？这是一个很重要的问题，毕竟不同的语言有不同的特性，选择不同的语言会对最终的结果产生很大的影响。

之前研究c协程库时，有看到大佬已经尝试过用c写动态链接库、然后java通过jni去调这种方式，最终失败了，具体原因得深入JVM源码才能得知，对鄙人来说太高深，告辞，因此排除java/kotlin等JVM字节码语言。

显然，用golang再去实现一个goroutine，且不说其复杂程度完全不亚于深入JVM源码，而且即使真的做出来，也不可能有人愿意在生产环境使用，因此排除golang。

到目前为止还剩下c/c++/rust 3位选手。

从研究过的好几个用c写的协程库来看，c的表达力差了点，需要编写巨量代码。相较之下，c++表达力就强多了，但开发的效率还是低了些，主要体现在以下几个方面：

1. `必须写cmake`。纯粹为了告诉系统怎么编译，有些麻烦，而这其实是不应该操心的部分；
2. `依赖管理麻烦`。如果要用别人写的类库，需要把代码拉下来，放到自己项目里，然后不得不耗费大量时间来通过编译。如果别人的库没有其他依赖还好，一旦有其他依赖，那么它依赖的依赖，也得按照刚才说的步骤处理，这就十分麻烦了；
3. `内存不安全`。c++很难写出没有内存泄漏/崩溃的代码。

<div style="text-align: center;">
    <img src="/docs/img/what_else_can_I_say.jpg" width="50%">
    <img src="/docs/img/rust.jpeg" width="100%">
</div>
