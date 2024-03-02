use crate::common::Current;
use crate::scheduler::SchedulerImpl;
use std::ffi::c_void;

thread_local! {
    static SCHEDULER: std::cell::RefCell<std::collections::VecDeque<*const c_void>> = const{std::cell::RefCell::new(std::collections::VecDeque::new())};
}

impl<'s> Current<'s> for SchedulerImpl<'s> {
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &Self)
    where
        Self: Sized,
    {
        SCHEDULER.with(|s| {
            s.borrow_mut()
                .push_front(std::ptr::from_ref(current) as *const c_void);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Named;
    use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender, Scheduler};

    #[test]
    fn test_current() -> std::io::Result<()> {
        let parent_name = "parent";
        let scheduler = SchedulerImpl::new(
            String::from(parent_name),
            crate::constants::DEFAULT_STACK_SIZE,
        );
        _ = scheduler.submit_co(
            move |_, _| {
                assert!(SchedulableCoroutine::current().is_some());
                assert!(SchedulableSuspender::current().is_some());
                assert_eq!(parent_name, SchedulerImpl::current().unwrap().get_name());
                assert_eq!(parent_name, SchedulerImpl::current().unwrap().get_name());

                let child_name = "child";
                let scheduler = SchedulerImpl::new(
                    String::from(child_name),
                    crate::constants::DEFAULT_STACK_SIZE,
                );
                _ = scheduler
                    .submit_co(
                        move |_, _| {
                            assert!(SchedulableCoroutine::current().is_some());
                            assert!(SchedulableSuspender::current().is_some());
                            assert_eq!(child_name, SchedulerImpl::current().unwrap().get_name());
                            assert_eq!(child_name, SchedulerImpl::current().unwrap().get_name());
                            None
                        },
                        None,
                    )
                    .unwrap();
                scheduler.try_schedule().unwrap();

                assert_eq!(parent_name, SchedulerImpl::current().unwrap().get_name());
                assert_eq!(parent_name, SchedulerImpl::current().unwrap().get_name());
                None
            },
            None,
        )?;
        scheduler.try_schedule()
    }
}
