use genawaiter::stack::Co;
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};

thread_local! {
    static DELAY_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
    static YIELDER: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
    static SYSCALL_FLAG: Box<RefCell<bool>> = Box::new(RefCell::new(false));
}

#[repr(transparent)]
pub struct Suspender<'y, Y, R = ()>(Co<'y, Y, R>);

impl<'y, Y, R> Suspender<'y, Y, R> {
    pub(crate) fn new(co: Co<'y, Y, R>) -> Self {
        Suspender(co)
    }

    /// Suspends the execution of a currently running coroutine.
    ///
    /// This function will switch control back to the original caller of
    /// [`Coroutine::resume`]. This function will then return once the
    /// [`Coroutine::resume`] function is called again.
    pub async fn suspend(&self, val: Y) -> R {
        let suspender = Suspender::<Y, R>::current();
        Suspender::<Y, R>::clean_current();
        let r = self.0.yield_(val).await;
        unsafe { Suspender::init_current(&mut *suspender) };
        r
    }

    pub async fn delay(&self, val: Y, ms_time: u64) -> R {
        self.delay_ns(
            val,
            match ms_time.checked_mul(1_000_000) {
                Some(v) => v,
                None => u64::MAX,
            },
        )
        .await
    }

    pub async fn delay_ns(&self, val: Y, ns_time: u64) -> R {
        Suspender::<Y, R>::init_delay_time(ns_time);
        self.suspend(val).await
    }

    fn init_current(yielder: &mut Suspender<Y, R>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *mut _ as *mut c_void;
        });
    }

    pub fn current<'s>() -> *mut Suspender<'s, Y, R> {
        YIELDER.with(|boxed| unsafe { std::mem::transmute(*boxed.borrow_mut()) })
    }

    fn clean_current() {
        YIELDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut())
    }

    fn init_delay_time(time: u64) {
        DELAY_TIME.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn delay_time() -> u64 {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_delay() {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }

    pub(crate) async fn syscall(&self, val: Y) -> R {
        Suspender::<Y, R>::init_syscall_flag();
        self.suspend(val).await
    }

    fn init_syscall_flag() {
        SYSCALL_FLAG.with(|boxed| {
            *boxed.borrow_mut() = true;
        });
    }

    pub(crate) fn syscall_flag() -> bool {
        SYSCALL_FLAG.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_syscall_flag() {
        SYSCALL_FLAG.with(|boxed| *boxed.borrow_mut() = false)
    }
}

impl<Y, R> Debug for Suspender<'_, Y, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suspender").finish()
    }
}
