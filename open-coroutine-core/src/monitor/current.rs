use crate::common::Current;
use crate::monitor::Monitor;
use std::ffi::c_void;

thread_local! {
    static MONITOR: std::cell::RefCell<std::collections::VecDeque<*const c_void>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

impl<'m> Current<'m> for Monitor {
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &Self)
    where
        Self: Sized,
    {
        MONITOR.with(|s| {
            s.borrow_mut()
                .push_front(std::ptr::from_ref(current) as *const c_void);
        });
    }

    fn current() -> Option<&'m Self>
    where
        Self: Sized,
    {
        MONITOR.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<Monitor>() })
        })
    }

    fn clean_current()
    where
        Self: Sized,
    {
        MONITOR.with(|s| _ = s.borrow_mut().pop_front());
    }
}
