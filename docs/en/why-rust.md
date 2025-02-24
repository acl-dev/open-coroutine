---
title: Language Selection
date: 2025-02-24 17:37:10
author: loongs-zhang
---

# Language Selection

English | [中文](../cn/why-rust.md)

What language is used to develop open routine? This is a very important issue, as different languages have different
features, and choosing different language can have a significant impact on the final outcome.

When researching the C coroutine library before, I saw that some experts had already tried to write dynamic link
libraries in C and call them in Java through JNI, but finally failed. The specific reason needs to be found in the
JVM source code, which is too hard for me, goodbye. So JVM bytecode languages such as Java/Kotlin are excluded.

Obviously, using Golang to implement a goroutine is no less complex than delving into JVM source code, and even if it is
actually finished, no one would be willing to use it in a production environment, so Golang is excluded.

Now, there are still three players left: c/c++/rust.

From several coroutine libraries written in C that have been studied, it can be seen that the expressiveness of C is a
bit lacking and requires writing a huge amount of code. In comparison, C++ has much stronger expressive power, but its
development efficiency is still low, mainly reflected in the following aspects:

1. `Have to write cmake`. Purely to tell the system how to compile, it's a bit troublesome, but this is actually the
   part that shouldn't be worried about;
2. `Difficulty in dependency management`. If you want to use a library written by someone else, you need to pull down
   the code and put it into your own project, and then you have to spend a lot of time compiling it. If the library has
   no other dependencies, it can barely be handled. Once there are other dependencies, the dependencies it depends on
   must also be handled according to the steps just mentioned, which can be very troublesome;
3. `Memory is unsafe`. It's difficult to write code in C++ without memory leaks/crashes.

<div style="text-align: center;">
    <img src="/docs/img/what_else_can_I_say.jpg" width="50%">
    <img src="/docs/img/rust.jpeg" width="100%">
</div>
