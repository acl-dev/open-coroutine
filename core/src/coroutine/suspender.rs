use crate::common::get_timeout_time;
use crate::impl_current_for;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static TIMESTAMP: RefCell<VecDeque<u64>> = const { RefCell::new(VecDeque::new()) };
}

impl<Param, Yield> Suspender<'_, Param, Yield> {
    /// Delay the execution of the coroutine with an arg after `Duration`.
    pub fn delay_with(&self, arg: Yield, delay: Duration) -> Param {
        self.until_with(arg, get_timeout_time(delay))
    }

    /// Delay the execution of the coroutine with an arg until `timestamp`.
    pub fn until_with(&self, arg: Yield, timestamp: u64) -> Param {
        TIMESTAMP.with(|s| {
            s.try_borrow_mut()
                .unwrap_or_else(|_| panic!("init TIMESTAMP current failed"))
                .push_front(timestamp);
        });
        self.suspend_with(arg)
    }

    pub(crate) fn timestamp() -> u64 {
        TIMESTAMP
            .with(|s| {
                s.try_borrow_mut()
                    .unwrap_or_else(|_| panic!("get TIMESTAMP current failed"))
                    .pop_front()
            })
            .unwrap_or(0)
    }
}

#[allow(clippy::must_use_candidate)]
impl<'s, Param> Suspender<'s, Param, ()> {
    /// see the `suspend_with` documents.
    pub fn suspend(&self) -> Param {
        self.suspend_with(())
    }

    /// see the `delay_with` documents.
    pub fn delay(&self, delay: Duration) -> Param {
        self.delay_with((), delay)
    }

    /// see the `until_with` documents.
    pub fn until(&self, timestamp: u64) -> Param {
        self.until_with((), timestamp)
    }
}

impl_current_for!(SUSPENDER, Suspender<'s, Param, Yield>);

#[cfg(feature = "korosensei")]
pub use korosensei::Suspender;
#[cfg(feature = "korosensei")]
mod korosensei {
    use corosensei::Yielder;

    /// Ths suspender implemented for coroutine.
    #[repr(C)]
    #[derive(educe::Educe)]
    #[educe(Debug)]
    pub struct Suspender<'s, Param, Yield> {
        #[educe(Debug(ignore))]
        inner: &'s Yielder<Param, Yield>,
    }

    impl<'s, Param, Yield> Suspender<'s, Param, Yield> {
        pub(crate) fn new(inner: &'s Yielder<Param, Yield>) -> Self {
            Self { inner }
        }

        /// Suspend the execution of current coroutine with an arg.
        pub fn suspend_with(&self, arg: Yield) -> Param {
            Self::clean_current();
            let param = self.inner.suspend(arg);
            Self::init_current(self);
            param
        }
    }
}
