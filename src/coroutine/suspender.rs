use corosensei::Yielder;
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};

#[repr(transparent)]
pub struct Suspender<'s, Param, Yield>(&'s Yielder<Param, Yield>);

thread_local! {
    static SUSPENDER: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl<'s, Param, Yield> Suspender<'s, Param, Yield> {
    pub(crate) fn new(yielder: &'s Yielder<Param, Yield>) -> Self {
        Suspender(yielder)
    }

    #[allow(clippy::ptr_as_ptr)]
    pub(crate) fn init_current(yielder: &Suspender<Param, Yield>) {
        SUSPENDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *const _ as *const c_void;
        });
    }

    #[must_use]
    pub fn current() -> &'s Suspender<'s, Param, Yield> {
        SUSPENDER
            .with(|boxed| unsafe { &*(*boxed.borrow_mut()).cast::<Suspender<'s, Param, Yield>>() })
    }

    pub(crate) fn clean_current() {
        SUSPENDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }
}

impl<'s, Param, Yield> Debug for Suspender<'s, Param, Yield> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suspender").finish()
    }
}
