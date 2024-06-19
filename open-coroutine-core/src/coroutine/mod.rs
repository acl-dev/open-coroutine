use crate::common::{Current, Named};
use crate::constants::CoroutineState;
use crate::coroutine::local::CoroutineLocal;
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
        $crate::coroutine::Coroutine::new(uuid::Uuid::new_v4().to_string(), $f, $size)
            .expect("create coroutine failed !")
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            uuid::Uuid::new_v4().to_string(),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
    ($name:expr, $f:expr, $size:expr $(,)?) => {
        $crate::coroutine::Coroutine::new($name, $f, $size).expect("create coroutine failed !")
    };
    ($name:expr, $f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new($name, $f, $crate::constants::DEFAULT_STACK_SIZE)
            .expect("create coroutine failed !")
    };
}

#[cfg(feature = "korosensei")]
pub use korosensei::Coroutine;

#[cfg(feature = "korosensei")]
mod korosensei;

#[cfg(all(feature = "boost", not(feature = "korosensei")))]
mod boost {}

#[cfg(test)]
mod tests;

impl<Param, Yield, Return> Coroutine<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + UnwindSafe,
    Return: Copy + UnwindSafe,
{
    /// Returns the current state of this `StateCoroutine`.
    pub fn state(&self) -> CoroutineState<Yield, Return> {
        self.state.get()
    }
}

impl<Yield, Return> Coroutine<'_, (), Yield, Return>
where
    Yield: Debug + Copy + UnwindSafe + Eq + PartialEq,
    Return: Debug + Copy + UnwindSafe + Eq + PartialEq,
{
    /// A simpler version of [`Coroutine::resume_with`].
    pub fn resume(&mut self) -> std::io::Result<CoroutineState<Yield, Return>> {
        self.resume_with(())
    }
}

impl<Param, Yield, Return> Coroutine<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Debug + Copy + UnwindSafe + Eq + PartialEq,
    Return: Debug + Copy + UnwindSafe + Eq + PartialEq,
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

impl<'c, Param, Yield, Return> Deref for Coroutine<'c, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + UnwindSafe,
    Return: Copy + UnwindSafe,
{
    type Target = CoroutineLocal<'c>;

    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

impl<Param, Yield, Return> Named for Coroutine<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Debug + Copy + UnwindSafe,
    Return: Debug + Copy + UnwindSafe,
{
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl_for_named!(
    Coroutine<'c, Param, Yield, Return>
    where
        Param: UnwindSafe,
        Yield: Copy + UnwindSafe,
        Return: Copy + UnwindSafe
);

impl_display_by_debug!(
    Coroutine<'c, Param, Yield, Return>
    where
        Param: UnwindSafe,
        Yield: Copy + UnwindSafe,
        Return: Copy + UnwindSafe
);

impl_current_for!(
    COROUTINE,
    Coroutine<'c, Param, Yield, Return>
    where
        Param: UnwindSafe,
        Yield: Copy + UnwindSafe,
        Return: Copy + UnwindSafe
);
