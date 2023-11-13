use crate::common::Current;
use crate::scheduler::SchedulerImpl;
use std::ffi::c_void;

thread_local! {
    static SCHEDULER: std::cell::RefCell<std::collections::VecDeque<*const c_void>> = std::cell::RefCell::new(std::collections::VecDeque::new());
}

impl<'s> Current<'s> for SchedulerImpl<'s> {
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &Self)
    where
        Self: Sized,
    {
        SCHEDULER.with(|s| {
            s.borrow_mut()
                .push_front(current as *const _ as *const c_void);
        });
    }

    fn current() -> Option<&'s Self>
    where
        Self: Sized,
    {
        SCHEDULER.with(|s| {
            s.borrow()
                .front()
                .map(|ptr| unsafe { &*(*ptr).cast::<SchedulerImpl<'s>>() })
        })
    }

    fn clean_current()
    where
        Self: Sized,
    {
        SCHEDULER.with(|s| _ = s.borrow_mut().pop_front());
    }
}
