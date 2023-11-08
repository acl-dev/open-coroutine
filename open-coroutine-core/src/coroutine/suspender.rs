use corosensei::Yielder;
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

#[repr(transparent)]
pub struct SuspenderImpl<'s, Param, Yield>(&'s Yielder<Param, Yield>);

thread_local! {
    static SUSPENDER: RefCell<*const c_void> = RefCell::new(std::ptr::null());
    static TIMESTAMP: RefCell<u64> = RefCell::new(0);
}

impl<'s, Param, Yield> SuspenderImpl<'s, Param, Yield> {
    pub(crate) fn new(yielder: &'s Yielder<Param, Yield>) -> Self {
        SuspenderImpl(yielder)
    }

    #[allow(clippy::ptr_as_ptr)]
    pub(crate) fn init_current(suspender: &SuspenderImpl<Param, Yield>) {
        SUSPENDER.with(|c| {
            _ = c.replace(suspender as *const _ as *const c_void);
        });
    }

    #[must_use]
    pub fn current() -> Option<&'s SuspenderImpl<'s, Param, Yield>> {
        SUSPENDER.with(|boxed| {
            let ptr = *boxed
                .try_borrow_mut()
                .expect("suspender current already borrowed");
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr).cast::<SuspenderImpl<'s, Param, Yield>>() })
            }
        })
    }

    pub(crate) fn clean_current() {
        SUSPENDER.with(|boxed| {
            *boxed
                .try_borrow_mut()
                .expect("suspender current already borrowed") = std::ptr::null();
        });
    }

    fn init_timestamp(time: u64) {
        TIMESTAMP.with(|c| {
            _ = c.replace(time);
        });
    }

    pub(crate) fn timestamp() -> u64 {
        TIMESTAMP.with(|boxed| boxed.replace(0))
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
