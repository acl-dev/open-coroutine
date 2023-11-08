use corosensei::Yielder;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

#[repr(transparent)]
pub struct SuspenderImpl<'s, Param, Yield>(&'s Yielder<Param, Yield>);

thread_local! {
    static SUSPENDER: RefCell<VecDeque<*const c_void>> = RefCell::new(VecDeque::new());
    static TIMESTAMP: RefCell<VecDeque<u64>> = RefCell::new(VecDeque::new());
}

impl<'s, Param, Yield> SuspenderImpl<'s, Param, Yield> {
    pub(crate) fn new(yielder: &'s Yielder<Param, Yield>) -> Self {
        SuspenderImpl(yielder)
    }

    #[allow(clippy::ptr_as_ptr)]
    pub(crate) fn init_current(current: &SuspenderImpl<Param, Yield>) {
        SUSPENDER.with(|s| {
            s.borrow_mut()
                .push_front(current as *const _ as *const c_void);
        });
    }

    #[must_use]
    pub fn current() -> Option<&'s SuspenderImpl<'s, Param, Yield>> {
        SUSPENDER.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<SuspenderImpl<'s, Param, Yield>>() })
        })
    }

    pub(crate) fn clean_current() {
        SUSPENDER.with(|s| _ = s.borrow_mut().pop_front());
    }

    fn init_timestamp(timestamp: u64) {
        TIMESTAMP.with(|s| {
            s.borrow_mut().push_front(timestamp);
        });
    }

    pub(crate) fn timestamp() -> u64 {
        TIMESTAMP.with(|s| s.borrow_mut().pop_front()).unwrap_or(0)
    }

    pub fn suspend_with(&self, val: Yield) -> Param {
        SuspenderImpl::<Param, Yield>::clean_current();
        let param = self.0.suspend(val);
        SuspenderImpl::<Param, Yield>::init_current(self);
        param
    }

    pub fn until_with(&self, val: Yield, timestamp: u64) -> Param {
        SuspenderImpl::<Param, Yield>::init_timestamp(timestamp);
        self.suspend_with(val)
    }

    pub fn delay_with(&self, val: Yield, time: Duration) -> Param {
        self.until_with(val, open_coroutine_timer::get_timeout_time(time))
    }
}

impl<'s> SuspenderImpl<'s, (), ()> {
    pub fn suspend(&self) {
        self.suspend_with(());
    }

    pub fn until(&self, timestamp: u64) {
        self.until_with((), timestamp);
    }

    pub fn delay(&self, time: Duration) {
        self.delay_with((), time);
    }
}

impl<'s, Param, Yield> Debug for SuspenderImpl<'s, Param, Yield> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suspender").finish()
    }
}
