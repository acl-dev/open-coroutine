use crate::common::{Current, Named};
use crate::constants::{CoroutineState, Syscall, SyscallState};
use crate::coroutine::local::HasCoroutineLocal;
use crate::coroutine::suspender::Suspender;
use std::cmp::Ordering;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::io::{Error, ErrorKind};
use std::panic::UnwindSafe;

/// Coroutine suspender abstraction.
pub mod suspender;

/// Coroutine local abstraction.
pub mod local;

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
pub trait Coroutine<'c>:
    Debug + Eq + PartialEq + Ord + PartialOrd + Named + Current<'c> + HasCoroutineLocal
{
    /// The type of value this coroutine accepts as a resume argument.
    type Resume: UnwindSafe;

    /// The type of value this coroutine yields.
    type Yield: Debug + Copy + Eq + PartialEq + UnwindSafe;

    /// The type of value this coroutine returns upon completion.
    type Return: Debug + Copy + Eq + PartialEq + UnwindSafe;

    /// Create a new coroutine.
    ///
    ///# Errors
    /// if stack allocate failed.
    fn new<F>(name: String, f: F, stack_size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(
            &dyn Suspender<Resume = Self::Resume, Yield = Self::Yield>,
            Self::Resume,
        ) -> Self::Return,
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

/// A trait implemented for describing changes in the state of the coroutine.
pub trait StateCoroutine<'c>: Coroutine<'c> {
    /// Returns the current state of this `StateCoroutine`.
    fn state(&self) -> CoroutineState<Self::Yield, Self::Return>;

    /// Returns the previous state of this `StateCoroutine`.
    /// Note: user should not use this method.
    fn change_state(
        &self,
        state: CoroutineState<Self::Yield, Self::Return>,
    ) -> CoroutineState<Self::Yield, Self::Return>;

    /// created -> ready
    /// suspend -> ready
    ///
    /// # Errors
    /// if change state fails.
    fn ready(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Created => {
                _ = self.change_state(CoroutineState::Ready);
                return Ok(());
            }
            CoroutineState::Suspend(_, timestamp) => {
                if timestamp <= open_coroutine_timer::now() {
                    _ = self.change_state(CoroutineState::Ready);
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::Ready
            ),
        ))
    }

    /// ready -> running
    /// syscall -> running
    ///
    /// below just for test
    /// created -> running
    /// suspend -> running
    ///
    /// # Errors
    /// if change state fails.
    fn running(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            #[cfg(test)]
            CoroutineState::Created => {
                _ = self.change_state(CoroutineState::Running);
                return Ok(());
            }
            CoroutineState::Ready => {
                _ = self.change_state(CoroutineState::Running);
                return Ok(());
            }
            // #[cfg(test)] preemptive.rs use this
            CoroutineState::Suspend(_, timestamp) => {
                if timestamp <= open_coroutine_timer::now() {
                    _ = self.change_state(CoroutineState::Running);
                    return Ok(());
                }
            }
            CoroutineState::SystemCall(
                _,
                _,
                SyscallState::Executing | SyscallState::Finished | SyscallState::Timeout,
            ) => {
                let state = CoroutineState::Running;
                _ = self.change_state(state);
                crate::info!("{} {:?}->{:?}", self.get_name(), current, state);
                return Ok(());
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::Running
            ),
        ))
    }

    /// running -> suspend
    ///
    /// # Errors
    /// if change state fails.
    fn suspend(&self, val: Self::Yield, timestamp: u64) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Suspend(val, timestamp));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::Suspend(val, timestamp)
            ),
        ))
    }

    /// running -> syscall
    /// inner: syscall -> syscall
    ///
    /// # Errors
    /// if change state fails.
    fn syscall(
        &self,
        val: Self::Yield,
        syscall: Syscall,
        syscall_state: SyscallState,
    ) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Running => {
                let state = CoroutineState::SystemCall(val, syscall, syscall_state);
                _ = self.change_state(state);
                crate::info!("{} {:?}->{:?}", self.get_name(), current, state);
                return Ok(());
            }
            CoroutineState::SystemCall(_, original_syscall, _) => {
                if original_syscall == syscall {
                    let state = CoroutineState::SystemCall(val, syscall, syscall_state);
                    _ = self.change_state(state);
                    crate::info!("{} {:?}->{:?}", self.get_name(), current, state);
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::SystemCall(
                    val,
                    syscall,
                    syscall_state
                )
            ),
        ))
    }

    /// running -> complete
    ///
    /// # Errors
    /// if change state fails.
    fn complete(&self, val: Self::Return) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Complete(val));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::Complete(val)
            ),
        ))
    }

    /// running -> error
    ///
    /// # Errors
    /// if change state fails.
    fn error(&self, val: &'static str) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Error(val));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Self::Yield, Self::Return>::Error(val)
            ),
        ))
    }
}

thread_local! {
    static COROUTINE: std::cell::RefCell<std::collections::VecDeque<
        *const c_void>> = std::cell::RefCell::new(std::collections::VecDeque::new());
}

impl<'c, Param, Yield, Return> Current<'c> for CoroutineImpl<'c, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &CoroutineImpl<'c, Param, Yield, Return>) {
        COROUTINE.with(|s| {
            s.borrow_mut()
                .push_front(current as *const _ as *const c_void);
        });
    }

    fn current() -> Option<&'c Self> {
        COROUTINE.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<CoroutineImpl<'c, Param, Yield, Return>>() })
        })
    }

    fn clean_current() {
        COROUTINE.with(|s| _ = s.borrow_mut().pop_front());
    }
}

impl<Param, Yield, Return> Debug for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe + Debug,
    Return: Copy + Eq + PartialEq + UnwindSafe + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.get_name())
            .field("status", &self.state())
            .field("local", self.local())
            .finish()
    }
}

impl<Param, Yield, Return> Eq for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
}

impl<Param, Yield, Return> PartialEq<Self> for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn eq(&self, other: &Self) -> bool {
        self.get_name().eq(other.get_name())
    }
}

impl<Param, Yield, Return> Ord for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_name().cmp(other.get_name())
    }
}

impl<Param, Yield, Return> PartialOrd<Self> for CoroutineImpl<'_, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
