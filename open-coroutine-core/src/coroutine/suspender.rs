use crate::common::Current;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::panic::UnwindSafe;
use std::time::Duration;

/// A trait implemented for suspend the execution of the coroutine.
pub trait Suspender<'s>: Current<'s> {
    /// The type of value this coroutine accepts as a resume argument.
    type Resume: UnwindSafe;

    /// The type of value the coroutine yields.
    type Yield: UnwindSafe;

    /// Suspend the execution of the coroutine.
    fn suspend_with(&self, arg: Self::Yield) -> Self::Resume;
}

thread_local! {
    static SUSPENDER: RefCell<VecDeque<*const c_void>> = RefCell::new(VecDeque::new());
}

impl<'s, Param, Yield> Current<'s> for SuspenderImpl<'s, Param, Yield>
where
    Param: UnwindSafe,
    Yield: UnwindSafe,
{
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &SuspenderImpl<'s, Param, Yield>) {
        SUSPENDER.with(|s| {
            s.borrow_mut()
                .push_front(current as *const _ as *const c_void);
        });
    }

    fn current() -> Option<&'s Self> {
        SUSPENDER.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<SuspenderImpl<'s, Param, Yield>>() })
        })
    }

    fn clean_current() {
        SUSPENDER.with(|s| _ = s.borrow_mut().pop_front());
    }
}

/// A trait implemented for suspend the execution of the coroutine.
pub trait SimpleSuspender<'s>: Suspender<'s, Yield = ()> {
    /// Suspend the execution of the coroutine.
    fn suspend(&self) -> Self::Resume;
}

impl<'s, SimpleSuspenderImpl: ?Sized + Suspender<'s, Yield = ()>> SimpleSuspender<'s>
    for SimpleSuspenderImpl
{
    fn suspend(&self) -> Self::Resume {
        self.suspend_with(())
    }
}

/// A trait implemented for suspend the execution of the coroutine.
pub trait DelaySuspender<'s>: Suspender<'s> {
    /// Delay the execution of the coroutine.
    fn delay_with(&self, arg: Self::Yield, delay: Duration) -> Self::Resume {
        self.until_with(arg, open_coroutine_timer::get_timeout_time(delay))
    }

    /// Delay the execution of the coroutine.
    fn until_with(&self, arg: Self::Yield, timestamp: u64) -> Self::Resume;

    /// When can a coroutine be resumed.
    fn timestamp() -> u64;
}

thread_local! {
    static TIMESTAMP: RefCell<VecDeque<u64>> = RefCell::new(VecDeque::new());
}

impl<'s, DelaySuspenderImpl: ?Sized + Suspender<'s>> DelaySuspender<'s> for DelaySuspenderImpl {
    fn until_with(&self, arg: Self::Yield, timestamp: u64) -> Self::Resume {
        TIMESTAMP.with(|s| {
            s.borrow_mut().push_front(timestamp);
        });
        self.suspend_with(arg)
    }

    fn timestamp() -> u64 {
        TIMESTAMP.with(|s| s.borrow_mut().pop_front()).unwrap_or(0)
    }
}

/// A trait implemented for suspend the execution of the coroutine.
pub trait SimpleDelaySuspender<'s>: DelaySuspender<'s, Yield = ()> {
    /// Delay the execution of the coroutine.
    fn delay(&self, delay: Duration) -> Self::Resume;

    /// Delay the execution of the coroutine.
    fn until(&self, timestamp: u64) -> Self::Resume;
}

impl<'s, SimpleDelaySuspenderImpl: ?Sized + DelaySuspender<'s, Yield = ()>> SimpleDelaySuspender<'s>
    for SimpleDelaySuspenderImpl
{
    fn delay(&self, delay: Duration) -> Self::Resume {
        self.delay_with((), delay)
    }

    fn until(&self, timestamp: u64) -> Self::Resume {
        self.until_with((), timestamp)
    }
}

#[cfg(feature = "korosensei")]
pub use korosensei::SuspenderImpl;
#[allow(missing_docs, missing_debug_implementations)]
#[cfg(feature = "korosensei")]
mod korosensei {
    use crate::common::Current;
    use crate::coroutine::suspender::Suspender;
    use corosensei::Yielder;
    use std::panic::UnwindSafe;

    #[repr(C)]
    pub struct SuspenderImpl<'s, Param, Yield>(pub(crate) &'s Yielder<Param, Yield>)
    where
        Param: UnwindSafe,
        Yield: UnwindSafe;

    impl<'s, Param, Yield> Suspender<'s> for SuspenderImpl<'s, Param, Yield>
    where
        Param: UnwindSafe,
        Yield: UnwindSafe,
    {
        type Resume = Param;
        type Yield = Yield;

        fn suspend_with(&self, arg: Self::Yield) -> Self::Resume {
            Self::clean_current();
            let param = self.0.suspend(arg);
            Self::init_current(self);
            param
        }
    }
}

#[allow(missing_docs, missing_debug_implementations)]
#[cfg(all(feature = "boost", not(feature = "corosensei")))]
mod boost {}
