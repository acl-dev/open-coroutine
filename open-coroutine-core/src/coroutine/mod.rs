use crate::common::{Current, Named};
use crate::constants::CoroutineState;
use crate::coroutine::suspender::Suspender;
use crate::{impl_current_for, impl_display_by_debug, impl_for_named};
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::panic::UnwindSafe;

/// Coroutine suspender abstraction.
pub mod suspender;

/// Coroutine local abstraction.
pub mod local;

/// Coroutine state abstraction and impl.
pub mod state;

/// Create a new coroutine.
#[macro_export]
macro_rules! co {
    ($f:expr, $size:literal $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(uuid::Uuid::new_v4().to_string(), $f, $size)
            .expect("create coroutine failed !")
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(
            uuid::Uuid::new_v4().to_string(),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
    ($name:expr, $f:expr, $size:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new($name, $f, $size).expect("create coroutine failed !")
    };
    ($name:expr, $f:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new($name, $f, $crate::constants::DEFAULT_STACK_SIZE)
            .expect("create coroutine failed !")
    };
}

use crate::coroutine::local::CoroutineLocal;
#[cfg(feature = "korosensei")]
pub use korosensei::CoroutineImpl;

#[allow(missing_docs)]
#[cfg(feature = "korosensei")]
mod korosensei;

#[cfg(all(feature = "boost", not(feature = "korosensei")))]
mod boost {}

#[cfg(test)]
mod tests;

/// A trait implemented for coroutines.
pub trait Coroutine<'c>: Debug + Named + Current + Deref<Target = CoroutineLocal<'c>> {
    /// The type of value this coroutine accepts as a resume argument.
    type Resume: UnwindSafe;

    /// The type of value this coroutine yields.
    type Yield: Debug + Copy + UnwindSafe;

    /// The type of value this coroutine returns upon completion.
    type Return: Debug + Copy + UnwindSafe;

    /// Create a new coroutine.
    ///
    ///# Errors
    /// if stack allocate failed.
    fn new<F>(name: String, f: F, stack_size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&Suspender<Self::Resume, Self::Yield>, Self::Resume) -> Self::Return,
        F: UnwindSafe,
        F: 'c,
        Self: Sized;

    /// Resumes the execution of this coroutine.
    ///
    /// The argument will be passed into the coroutine as a resume argument.
    ///
    /// # Errors
    /// if current coroutine state is unexpected.
    fn resume_with(
        &mut self,
        arg: Self::Resume,
    ) -> std::io::Result<CoroutineState<Self::Yield, Self::Return>>;
}

/// A trait implemented for coroutines when Resume is ().
pub trait SimpleCoroutine<'c>: Coroutine<'c, Resume = ()> {
    /// Resumes the execution of this coroutine.
    ///
    /// # Errors
    /// see `resume_with`
    fn resume(&mut self) -> std::io::Result<CoroutineState<Self::Yield, Self::Return>>;
}

impl<'c, SimpleCoroutineImpl: Coroutine<'c, Resume = ()>> SimpleCoroutine<'c>
    for SimpleCoroutineImpl
{
    fn resume(&mut self) -> std::io::Result<CoroutineState<Self::Yield, Self::Return>> {
        self.resume_with(())
    }
}

impl_current_for!(
    COROUTINE,
    CoroutineImpl<'c, Param: UnwindSafe, Yield: Copy + UnwindSafe, Return: Copy + UnwindSafe>
);

impl<Param, Yield, Return> Debug for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + UnwindSafe + Debug,
    Return: Copy + UnwindSafe + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.get_name())
            .field("status", &self.state())
            .field("local", &self.local)
            .finish()
    }
}

impl_display_by_debug!(
    CoroutineImpl<'c, Param: UnwindSafe, Yield: Copy + UnwindSafe, Return: Copy + UnwindSafe>
);

impl_for_named!(
    CoroutineImpl<'c, Param: UnwindSafe, Yield: Copy + UnwindSafe, Return: Copy + UnwindSafe>
);
