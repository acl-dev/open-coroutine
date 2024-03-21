use crate::common::Current;
use crate::pool::CoroutinePoolImpl;
use std::ffi::c_void;

thread_local! {
    static COROUTINE_POOL: std::cell::RefCell<std::collections::VecDeque<*const c_void>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

impl<'p> Current<'p> for CoroutinePoolImpl<'p> {
    #[allow(clippy::ptr_as_ptr)]
    fn init_current(current: &Self)
    where
        Self: Sized,
    {
        COROUTINE_POOL.with(|s| {
            s.borrow_mut()
                .push_front(std::ptr::from_ref(current) as *const c_void);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Named;
    use crate::constants::DEFAULT_STACK_SIZE;
    use crate::pool::{CoroutinePool, TaskPool};
    use crate::scheduler::{SchedulableCoroutine, SchedulableSuspender, SchedulerImpl};

    #[test]
    fn test_current() -> std::io::Result<()> {
        let parent_name = "parent";
        let pool = CoroutinePoolImpl::new(
            String::from(parent_name),
            1,
            DEFAULT_STACK_SIZE,
            0,
            65536,
            0,
            crate::common::DelayBlocker::default(),
        );
        _ = pool.submit(
            None,
            |_, _| {
                assert!(SchedulableCoroutine::current().is_some());
                assert!(SchedulableSuspender::current().is_some());
                assert!(SchedulerImpl::current().is_some());
                assert_eq!(
                    parent_name,
                    CoroutinePoolImpl::current().unwrap().get_name()
                );
                assert_eq!(
                    parent_name,
                    CoroutinePoolImpl::current().unwrap().get_name()
                );

                let child_name = "child";
                let pool = CoroutinePoolImpl::new(
                    String::from(child_name),
                    1,
                    DEFAULT_STACK_SIZE,
                    0,
                    65536,
                    0,
                    crate::common::DelayBlocker::default(),
                );
                _ = pool.submit(
                    None,
                    |_, _| {
                        assert!(SchedulableCoroutine::current().is_some());
                        assert!(SchedulableSuspender::current().is_some());
                        assert!(SchedulerImpl::current().is_some());
                        assert_eq!(child_name, CoroutinePoolImpl::current().unwrap().get_name());
                        assert_eq!(child_name, CoroutinePoolImpl::current().unwrap().get_name());
                        None
                    },
                    None,
                );
                pool.try_schedule_task().unwrap();

                assert_eq!(
                    parent_name,
                    CoroutinePoolImpl::current().unwrap().get_name()
                );
                assert_eq!(
                    parent_name,
                    CoroutinePoolImpl::current().unwrap().get_name()
                );
                None
            },
            None,
        );
        pool.try_schedule_task()
    }
}
