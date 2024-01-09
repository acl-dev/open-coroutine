use crate::constants::{Syscall, SyscallState};
use crate::scheduler::{SchedulableCoroutine, SchedulerImpl};
use std::fmt::Debug;
use std::panic::AssertUnwindSafe;

/// A trait implemented for schedulers, mainly used for monitoring.
pub trait Listener: Debug {
    /// callback when a coroutine is created.
    /// This will be called by `Scheduler` when a coroutine is created.
    fn on_create(&self, _: &SchedulableCoroutine) {}

    /// callback before scheduling coroutines.
    /// This will be called by `Scheduler` before scheduling coroutines.
    fn on_schedule(&self, _: u64) {}

    /// callback before resuming the coroutine.
    /// This will be called by `Scheduler` before resuming the coroutine.
    fn on_resume(&self, _: u64, _: &SchedulableCoroutine) {}

    /// callback when a coroutine is suspended.
    /// This will be called by `Scheduler` when a coroutine is suspended.
    fn on_suspend(&self, _: u64, _: &SchedulableCoroutine) {}

    /// callback when a coroutine enters syscall.
    /// This will be called by `Scheduler` when a coroutine enters syscall.
    fn on_syscall(&self, _: u64, _: &SchedulableCoroutine, _: Syscall, _: SyscallState) {}

    /// callback when a coroutine is completed.
    /// This will be called by `Scheduler` when a coroutine is completed.
    fn on_complete(&self, _: u64, _: &SchedulableCoroutine, _: Option<usize>) {}

    /// callback when a coroutine is panic.
    /// This will be called by `Scheduler` when a coroutine is panic.
    fn on_error(&self, _: u64, _: &SchedulableCoroutine, _: &str) {}
}

#[allow(box_pointers)]
impl Listener for SchedulerImpl<'_> {
    fn on_create(&self, coroutine: &SchedulableCoroutine) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_create(coroutine);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_create error:{}", listener, message);
            }
        }
    }

    fn on_schedule(&self, timeout_time: u64) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_schedule(timeout_time);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_schedule error:{}", listener, message);
            }
        }
    }

    fn on_resume(&self, timeout_time: u64, coroutine: &SchedulableCoroutine) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_resume(timeout_time, coroutine);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_resume error:{}", listener, message);
            }
        }
    }

    fn on_suspend(&self, timeout_time: u64, coroutine: &SchedulableCoroutine) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_suspend(timeout_time, coroutine);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_suspend error:{}", listener, message);
            }
        }
    }

    fn on_syscall(
        &self,
        timeout_time: u64,
        coroutine: &SchedulableCoroutine,
        syscall: Syscall,
        state: SyscallState,
    ) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_syscall(timeout_time, coroutine, syscall, state);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_syscall error:{}", listener, message);
            }
        }
    }

    fn on_complete(
        &self,
        timeout_time: u64,
        coroutine: &SchedulableCoroutine,
        result: Option<usize>,
    ) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_complete(timeout_time, coroutine, result);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_complete error:{}", listener, message);
            }
        }
    }

    fn on_error(&self, timeout_time: u64, coroutine: &SchedulableCoroutine, message: &str) {
        for listener in &self.listeners {
            if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(|| {
                listener.on_error(timeout_time, coroutine, message);
            })) {
                #[cfg(feature = "logs")]
                let message = *e
                    .downcast_ref::<&'static str>()
                    .unwrap_or(&"Listener failed without message");
                crate::error!("{:?} on_error error:{}", listener, message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Scheduler;

    #[derive(Debug, Default)]
    struct TestListener {}
    impl Listener for TestListener {
        fn on_create(&self, coroutine: &SchedulableCoroutine) {
            println!("{:?}", coroutine);
        }
        fn on_resume(&self, _: u64, coroutine: &SchedulableCoroutine) {
            println!("{:?}", coroutine);
        }
        fn on_complete(&self, _: u64, coroutine: &SchedulableCoroutine, result: Option<usize>) {
            println!("{:?} {:?}", coroutine, result);
        }
        fn on_error(&self, _: u64, coroutine: &SchedulableCoroutine, message: &str) {
            println!("{:?} {message}", coroutine);
        }
    }

    #[test]
    fn test_listener() -> std::io::Result<()> {
        let mut scheduler = SchedulerImpl::default();
        scheduler.add_listener(TestListener::default());
        _ = scheduler.submit_co(|_, _| panic!("test panic, just ignore it"), None)?;
        _ = scheduler.submit_co(
            |_, _| {
                println!("2");
                Some(1)
            },
            None,
        )?;
        scheduler.try_schedule()
    }
}
