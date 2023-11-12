use crate::common::Current;
use crate::pool::CoroutinePoolImpl;
use std::ffi::c_void;

thread_local! {
    static COROUTINE_POOL: std::cell::RefCell<std::collections::VecDeque<*const c_void>> = std::cell::RefCell::new(std::collections::VecDeque::new());
}

impl<'p> Current<'p> for CoroutinePoolImpl<'p> {
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &Self)
    where
        Self: Sized,
    {
        COROUTINE_POOL.with(|s| {
            s.borrow_mut()
                .push_front(current as *const _ as *const c_void);
        });
    }

    fn current() -> Option<&'p Self>
    where
        Self: Sized,
    {
        COROUTINE_POOL.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<CoroutinePoolImpl<'p>>() })
        })
    }

    fn clean_current()
    where
        Self: Sized,
    {
        COROUTINE_POOL.with(|s| _ = s.borrow_mut().pop_front());
    }
}
