#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

/// A `Context` stores a `ContextFn`'s state of execution, for it to be resumed later.
///
/// If we have 2 or more `Context` instances, we can thus easily "freeze" the
/// current state of execution and explicitely switch to another `Context`.
/// This `Context` is then resumed exactly where it left of and
/// can in turn "freeze" and switch to another `Context`.
///
/// # Examples
///
/// See [examples/basic.rs](https://github.com/zonyitoo/context-rs/blob/master/examples/basic.rs)
struct Context {
  const void *_0;
};

/// Contains the previously active `Context` and the `data` passed to resume the current one and
/// is used as the return value by `Context::resume()` and `Context::resume_ontop()`
struct Transfer {
  /// The previously executed `Context` which yielded to resume the current one.
  Context context;
  /// The `data` which was passed to `Context::resume()` or
  /// `Context::resume_ontop()` to resume the current `Context`.
  uintptr_t data;
};

template<typename Param, typename Yield, typename Return>
using Yielder = const Transfer*;

using UserFunction = void*(*)(const Yielder<void*, void, void*>*, void*);

/// Functions of this signature are used as the entry point for a new `Context`.
using ContextFn = void(*)(Transfer t);

extern "C" {

///创建协程
int coroutine_crate(UserFunction f, void *param, uintptr_t stack_size);

void *suspend(const Yielder<void*, void, void*> *yielder);

void *delay(const Yielder<void*, void, void*> *yielder, uint64_t ms_time);

///轮询协程
int try_timed_schedule(uint64_t ms_time);

int try_timeout_schedule(uint64_t timeout_time);

/// Creates a new `Context` ontop of some stack.
///
/// # Arguments
/// * `sp`   - A pointer to the bottom of the stack.
/// * `size` - The size of the stack.
/// * `f`    - A function to be invoked on the first call to jump_fcontext(this, _).
extern const void *make_fcontext(void *sp, uintptr_t size, ContextFn f);

/// Yields the execution to another `Context`.
///
/// # Arguments
/// * `to` - A pointer to the `Context` with whom we swap execution.
/// * `p`  - An arbitrary argument that will be set as the `data` field
///          of the `Transfer` object passed to the other Context.
extern Transfer jump_fcontext(const void *to, uintptr_t p);

} // extern "C"
