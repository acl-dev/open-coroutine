use crate::common::constants::CoroutineState;
use crate::common::ordered_work_steal::Ordered;
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::{impl_current_for, impl_display_by_debug, impl_for_named};
use std::collections::VecDeque;
use std::ffi::c_longlong;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

/// Coroutine suspender abstraction and impl.
pub mod suspender;

/// Coroutine local abstraction.
pub mod local;

/// Coroutine listener abstraction and impl.
pub mod listener;

/// Reuse stacks.
pub mod stack_pool;

#[cfg(feature = "korosensei")]
pub use korosensei::Coroutine;
#[cfg(feature = "korosensei")]
mod korosensei;

/// Create a new coroutine.
#[macro_export]
macro_rules! co {
    ($name:expr, $f:expr, $size:expr, $priority:expr $(,)?) => {
        $crate::coroutine::Coroutine::new($name, $f, $size, $priority)
    };
    ($f:expr, $size:literal, $priority:literal $(,)?) => {
        $crate::coroutine::Coroutine::new(
            uuid::Uuid::new_v4().to_string(),
            $f,
            $size,
            Some($priority),
        )
    };
    ($name:expr, $f:expr, $size:expr $(,)?) => {
        $crate::coroutine::Coroutine::new($name, $f, $size, None)
    };
    ($f:expr, $size:literal $(,)?) => {
        $crate::coroutine::Coroutine::new(uuid::Uuid::new_v4().to_string(), $f, $size, None)
    };
    ($name:expr, $f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            $name,
            $f,
            $crate::common::constants::DEFAULT_STACK_SIZE,
            None,
        )
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            uuid::Uuid::new_v4().to_string(),
            $f,
            $crate::common::constants::DEFAULT_STACK_SIZE,
            None,
        )
    };
}

/// The coroutine stack information.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct StackInfo {
    /// The base address of the stack. This is the highest address
    /// since stacks grow downwards on most modern architectures.
    pub stack_top: usize,
    /// The maximum limit address of the stack. This is the lowest address
    /// since stacks grow downwards on most modern architectures.
    pub stack_bottom: usize,
}

/// Coroutine state abstraction and impl.
mod state;

impl<'c, Param, Yield, Return> Coroutine<'c, Param, Yield, Return> {
    /// Get the name of this coroutine.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the current state of this `StateCoroutine`.
    pub fn state(&self) -> CoroutineState<Yield, Return>
    where
        Yield: Copy,
        Return: Copy,
    {
        self.state.get()
    }

    /// Add a listener to this coroutine.
    pub fn add_listener(&mut self, listener: impl Listener<Yield, Return> + 'c) {
        self.add_raw_listener(Box::leak(Box::new(listener)));
    }

    /// Queries the amount of remaining stack as interpreted by this coroutine.
    ///
    /// This function will return the amount of stack space left which will be used
    /// to determine whether a stack switch should be made or not.
    ///
    /// # Safety
    ///
    /// This can only be done safely in coroutine.
    pub unsafe fn remaining_stack(&self) -> usize {
        let current_ptr = psm::stack_pointer() as usize;
        current_ptr - self.stack_infos.borrow().back().unwrap().stack_bottom
    }

    /// Queries the current stack info of this coroutine.
    ///
    /// The first used stack index is 0 and increases with usage.
    pub fn stack_infos(&self) -> VecDeque<StackInfo> {
        self.stack_infos.borrow().clone()
    }

    /// Checks whether the stack pointer at the point where a trap occurred is
    /// within the coroutine that this `CoroutineTrapHandler` was produced from.
    /// This check includes any guard pages on the stack and will therefore
    /// still return true in the case of a stack overflow.
    ///
    /// The result of this function is only meaningful if the coroutine has not
    /// been dropped yet.
    pub fn stack_ptr_in_bounds(&self, stack_ptr: u64) -> bool {
        for info in self.stack_infos.borrow().iter() {
            if info.stack_bottom as u64 <= stack_ptr && stack_ptr < info.stack_top as u64 {
                return true;
            }
        }
        false
    }

    /// Grows the call stack if necessary.
    ///
    /// This function is intended to be called at manually instrumented points in a program where
    /// recursion is known to happen quite a bit. This function will check to see if we're within
    /// `32 * 1024` bytes of the end of the stack, and if so it will allocate a new stack of at least
    /// `128 * 1024` bytes.
    ///
    /// The closure `f` is guaranteed to run on a stack with at least `32 * 1024` bytes, and it will be
    /// run on the current stack if there's space available.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn maybe_grow<R, F: FnOnce() -> R>(callback: F) -> std::io::Result<R> {
        Self::maybe_grow_with(
            crate::common::default_red_zone(),
            crate::common::constants::DEFAULT_STACK_SIZE,
            callback,
        )
    }
}

impl<Yield, Return> Coroutine<'_, (), Yield, Return>
where
    Yield: Debug + Copy + Eq + 'static,
    Return: Debug + Copy + Eq + 'static,
{
    /// A simpler version of [`Coroutine::resume_with`].
    pub fn resume(&mut self) -> std::io::Result<CoroutineState<Yield, Return>> {
        self.resume_with(())
    }
}

impl<Param, Yield, Return> Coroutine<'_, Param, Yield, Return>
where
    Param: 'static,
    Yield: Debug + Copy + Eq + 'static,
    Return: Debug + Copy + Eq + 'static,
{
    /// Resumes the execution of this coroutine.
    ///
    /// The argument will be passed into the coroutine as a resume argument.
    ///
    /// # Errors
    /// if current coroutine state is unexpected.
    pub fn resume_with(&mut self, arg: Param) -> std::io::Result<CoroutineState<Yield, Return>> {
        let current = self.state();
        if let CoroutineState::Complete(r) = current {
            return Ok(CoroutineState::Complete(r));
        }
        if let CoroutineState::Error(e) = current {
            return Ok(CoroutineState::Error(e));
        }
        Self::init_current(self);
        self.running()?;
        let r = self.raw_resume(arg);
        Self::clean_current();
        r
    }
}

impl<Param, Yield, Return> Debug for Coroutine<'_, Param, Yield, Return>
where
    Yield: Debug + Copy,
    Return: Debug + Copy,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name())
            .field("state", &self.state())
            .field("stack_infos", &self.stack_infos)
            .field("local", &self.local)
            .field("priority", &self.priority)
            .finish()
    }
}

impl<'c, Param, Yield, Return> Deref for Coroutine<'c, Param, Yield, Return> {
    type Target = CoroutineLocal<'c>;

    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

impl<Param, Yield, Return> Ordered for Coroutine<'_, Param, Yield, Return> {
    fn priority(&self) -> Option<c_longlong> {
        self.priority
    }
}

impl_display_by_debug!(
    Coroutine<'c, Param, Yield, Return>
    where
        Yield: Debug + Copy,
        Return: Debug + Copy
);

impl_for_named!(Coroutine<'c, Param, Yield, Return>);

impl_current_for!(COROUTINE, Coroutine<'c, Param, Yield, Return>);
