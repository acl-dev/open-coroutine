use crate::common::get_timeout_time;
use crate::impl_current_for;
use std::time::Duration;

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static TIMESTAMP: crossbeam_utils::atomic::AtomicCell<std::collections::VecDeque<u64>> =
        const { crossbeam_utils::atomic::AtomicCell::new(std::collections::VecDeque::new()) };

    #[allow(clippy::missing_const_for_thread_local)]
    static CANCEL: crossbeam_utils::atomic::AtomicCell<std::collections::VecDeque<bool>> =
        const { crossbeam_utils::atomic::AtomicCell::new(std::collections::VecDeque::new()) };
}

impl<Param, Yield> Suspender<'_, Param, Yield> {
    /// Delay the execution of the coroutine with an arg after `Duration`.
    pub fn delay_with(&self, arg: Yield, delay: Duration) -> Param {
        self.until_with(arg, get_timeout_time(delay))
    }

    /// Delay the execution of the coroutine with an arg until `timestamp`.
    pub fn until_with(&self, arg: Yield, timestamp: u64) -> Param {
        TIMESTAMP.with(|s| unsafe {
            s.as_ptr()
                .as_mut()
                .unwrap_or_else(|| {
                    panic!(
                        "thread:{} init TIMESTAMP current failed",
                        std::thread::current().name().unwrap_or("unknown")
                    )
                })
                .push_front(timestamp);
        });
        self.suspend_with(arg)
    }

    pub(crate) fn timestamp() -> u64 {
        TIMESTAMP
            .with(|s| unsafe {
                s.as_ptr()
                    .as_mut()
                    .unwrap_or_else(|| {
                        panic!(
                            "thread:{} get TIMESTAMP current failed",
                            std::thread::current().name().unwrap_or("unknown")
                        )
                    })
                    .pop_front()
            })
            .unwrap_or(0)
    }

    /// Cancel the execution of the coroutine.
    pub fn cancel(&self) -> ! {
        CANCEL.with(|s| unsafe {
            s.as_ptr()
                .as_mut()
                .unwrap_or_else(|| {
                    panic!(
                        "thread:{} init CANCEL current failed",
                        std::thread::current().name().unwrap_or("unknown")
                    )
                })
                .push_front(true);
        });
        _ = self.suspend_with(unsafe { std::mem::zeroed() });
        unreachable!()
    }

    pub(crate) fn is_cancel() -> bool {
        CANCEL
            .with(|s| unsafe {
                s.as_ptr()
                    .as_mut()
                    .unwrap_or_else(|| {
                        panic!(
                            "thread:{} get CANCEL current failed",
                            std::thread::current().name().unwrap_or("unknown")
                        )
                    })
                    .pop_front()
            })
            .unwrap_or(false)
    }
}

#[allow(clippy::must_use_candidate)]
impl<Param> Suspender<'_, Param, ()> {
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
    #[educe(Debug(named_field = false))]
    pub struct Suspender<'s, Param, Yield> {
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
