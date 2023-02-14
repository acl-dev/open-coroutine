// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use crate::stack::Stack;
use std::fmt;
use std::os::raw::c_void;

// Requires cdecl calling convention on x86, which is the default for "C" blocks.
extern "C" {
    /// Creates a new `Context` ontop of some stack.
    ///
    /// # Arguments
    /// * `sp`   - A pointer to the bottom of the stack.
    /// * `size` - The size of the stack.
    /// * `f`    - A function to be invoked on the first call to jump_fcontext(this, _).
    fn make_fcontext(sp: *mut c_void, size: usize, f: ContextFn) -> &'static c_void;

    /// Yields the execution to another `Context`.
    ///
    /// # Arguments
    /// * `to` - A pointer to the `Context` with whom we swap execution.
    /// * `p`  - An arbitrary argument that will be set as the `data` field
    ///          of the `Transfer` object passed to the other Context.
    fn jump_fcontext(to: &'static c_void, p: usize) -> Transfer;
}

/// Functions of this signature are used as the entry point for a new `Context`.
pub type ContextFn = extern "C" fn(t: Transfer);

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
// The reference is using 'static because we can't possibly imply the
// lifetime of the Context instances returned by resume() anyways.
#[repr(C)]
pub struct Context(&'static c_void);

// NOTE: Rustc is kinda dumb and introduces a overhead of up to 500% compared to the asm methods
//       if we don't explicitely inline them or use LTO (e.g.: 3ns/iter VS. 18ns/iter on i7 3770).
impl Context {
    /// Creates a new `Context` prepared to execute `f` at the beginning of `stack`.
    ///
    /// `f` is not executed until the first call to `resume()`.
    ///
    /// It is unsafe because it only takes a reference of `Stack`. You have to make sure the
    /// `Stack` lives longer than the generated `Context`.
    #[inline(always)]
    pub unsafe fn new(stack: &Stack, f: ContextFn) -> Context {
        Context(make_fcontext(stack.top(), stack.len(), f))
    }

    /// Yields the execution to another `Context`.
    ///
    /// The exact behaviour of this method is implementation defined, but the general mechanism is:
    /// The current state of execution is preserved somewhere and the previously saved state
    /// in the `Context` pointed to by `self` is restored and executed next.
    ///
    /// This behaviour is similiar in spirit to regular function calls with the difference
    /// that the call to `resume()` only returns when someone resumes the caller in turn.
    ///
    /// The returned `Transfer` struct contains the previously active `Context` and
    /// the `data` argument used to resume the current one.
    ///
    /// It is unsafe because it is your responsibility to make sure that all data that constructed in
    /// this context have to be dropped properly when the last context is dropped.
    #[inline(always)]
    pub unsafe fn resume(&self, data: usize) -> Transfer {
        #[cfg(feature = "stack-trace")]
        crate::stack_trace::add_backtrace();
        //actually, we call the jump_fcontext function
        jump_fcontext(self.0, data)
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Context({:p})", self.0)
    }
}

/// Contains the previously active `Context` and the `data` passed to resume the current one and
/// is used as the return value by `Context::resume()` and `Context::resume_ontop()`
#[repr(C)]
#[derive(Debug)]
pub struct Transfer {
    /// The previously executed `Context` which yielded to resume the current one.
    pub context: Context,

    /// The `data` which was passed to `Context::resume()` or
    /// `Context::resume_ontop()` to resume the current `Context`.
    pub data: usize,
}

impl Transfer {
    /// Returns a new `Transfer` struct with the members set to their respective arguments.
    #[inline(always)]
    pub fn new(context: Context, data: usize) -> Transfer {
        Transfer { context, data }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::os::raw::c_void;

    use super::*;
    use crate::stack::ProtectedFixedSizeStack;

    #[test]
    fn type_sizes() {
        assert_eq!(mem::size_of::<Context>(), mem::size_of::<usize>());
        assert_eq!(mem::size_of::<Context>(), mem::size_of::<*const c_void>());
    }

    #[test]
    fn number_generator() {
        extern "C" fn context_function(mut t: Transfer) {
            for i in 0usize.. {
                assert_eq!(t.data, i);
                t = unsafe { t.context.resume(i) };
            }

            unreachable!();
        }

        let stack = ProtectedFixedSizeStack::default();
        let mut t = Transfer::new(unsafe { Context::new(&stack, context_function) }, 0);

        for i in 0..10usize {
            t = unsafe { t.context.resume(i) };
            assert_eq!(t.data, i);

            if t.data == 9 {
                break;
            }
        }
    }
}
