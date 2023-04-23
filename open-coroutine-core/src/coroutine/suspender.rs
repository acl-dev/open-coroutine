use corosensei::Yielder;
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

#[repr(transparent)]
pub struct Suspender<'s, Param, Yield>(&'s Yielder<Param, Yield>);

thread_local! {
    static SUSPENDER: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
    static TIMESTAMP: Box<RefCell<u64>> = Box::new(RefCell::new(0));
    static SYSCALL: Box<RefCell<&'static str>> = Box::new(RefCell::new(""));
}

impl<'s, Param, Yield> Suspender<'s, Param, Yield> {
    pub(crate) fn new(yielder: &'s Yielder<Param, Yield>) -> Self {
        Suspender(yielder)
    }

    #[allow(clippy::ptr_as_ptr)]
    pub(crate) fn init_current(suspender: &Suspender<Param, Yield>) {
        SUSPENDER.with(|boxed| {
            *boxed.borrow_mut() = suspender as *const _ as *const c_void;
        });
    }

    #[must_use]
    pub fn current() -> Option<&'s Suspender<'s, Param, Yield>> {
        SUSPENDER.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr).cast::<Suspender<'s, Param, Yield>>() })
            }
        })
    }

    pub(crate) fn clean_current() {
        SUSPENDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    fn init_timestamp(time: u64) {
        TIMESTAMP.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn timestamp() -> u64 {
        TIMESTAMP.with(|boxed| {
            let val = *boxed.borrow_mut();
            *boxed.borrow_mut() = 0;
            val
        })
    }

    fn init_syscall_name(syscall_name: &str) {
        SYSCALL.with(|boxed| {
            *boxed.borrow_mut() = Box::leak(Box::from(syscall_name));
        });
    }

    pub(crate) fn syscall_name() -> &'static str {
        SYSCALL.with(|boxed| {
            let val = *boxed.borrow_mut();
            *boxed.borrow_mut() = "";
            val
        })
    }

    pub fn suspend_with(&self, val: Yield) -> Param {
        Suspender::<Param, Yield>::clean_current();
        let param = self.0.suspend(val);
        Suspender::<Param, Yield>::init_current(self);
        param
    }

    pub fn until_with(&self, val: Yield, timestamp: u64) -> Param {
        Suspender::<Param, Yield>::init_timestamp(timestamp);
        self.suspend_with(val)
    }

    pub fn delay_with(&self, val: Yield, time: Duration) -> Param {
        self.until_with(val, timer_utils::get_timeout_time(time))
    }

    //只有框架级crate才需要使用此方法
    pub fn syscall_with(&self, val: Yield, syscall_name: &str) -> Param {
        Suspender::<Param, Yield>::init_syscall_name(syscall_name);
        self.suspend_with(val)
    }
}

impl<'s> Suspender<'s, (), ()> {
    pub fn suspend(&self) {
        self.suspend_with(());
    }

    pub fn until(&self, timestamp: u64) {
        self.until_with((), timestamp);
    }

    pub fn delay(&self, time: Duration) {
        self.delay_with((), time);
    }

    //只有框架级crate才需要使用此方法
    pub fn syscall(&self, syscall_name: &str) {
        self.syscall_with((), syscall_name);
    }
}

impl<'s, Param, Yield> Debug for Suspender<'s, Param, Yield> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suspender").finish()
    }
}
