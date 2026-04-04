use crate::common::constants::{CoroutineState, SyscallName, SyscallState};
use crate::common::now;
use crate::coroutine::listener::Listener;
use crate::coroutine::Coroutine;
use crate::{error, info};
use std::fmt::Debug;
use std::io::Error;

impl<Param, Yield, Return> Coroutine<'_, Param, Yield, Return>
where
    Yield: Debug + Copy + Eq,
    Return: Debug + Copy + Eq,
{
    /// Returns the previous state of this `StateCoroutine`.
    /// Note: user should not use this method.
    fn change_state(
        &self,
        new_state: CoroutineState<Yield, Return>,
    ) -> CoroutineState<Yield, Return> {
        let old_state = self.state.replace(new_state);
        //对→Running的转换：先记录日志再通知MonitorListener。
        //on_state_changed(Running)会通过MonitorListener设置10ms的NOTIFY_NODE定时器，
        //如果先通知再记录日志，在QEMU等慢平台上info!()可能耗时>10ms，
        //导致定时器在日志记录期间过期→SIGURG→抢占活锁。
        //先记录日志确保NOTIFY_NODE定时器在日志I/O完成后才启动。
        // For →Running transitions: log BEFORE notifying MonitorListener.
        // on_state_changed(Running) sets a 10ms NOTIFY_NODE timer via MonitorListener.
        // If notified first, info!() can take >10ms on slow platforms (QEMU),
        // causing the timer to expire during logging → SIGURG → preemption live-lock.
        // Logging first ensures the NOTIFY_NODE timer starts after slow I/O completes.
        if matches!(new_state, CoroutineState::Running) {
            info!("{} {:?}->{:?}", self.name(), old_state, new_state);
            self.on_state_changed(self, old_state, new_state);
        } else {
            self.on_state_changed(self, old_state, new_state);
            if let CoroutineState::Error(_) = new_state {
                error!("{} {:?}->{:?}", self.name(), old_state, new_state);
            } else {
                info!("{} {:?}->{:?}", self.name(), old_state, new_state);
            }
        }
        old_state
    }

    /// suspend -> ready
    ///
    /// # Errors
    /// if change state fails.
    pub(crate) fn ready(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Ready => return Ok(()),
            CoroutineState::Suspend(_, timestamp) if timestamp <= now() => {
                let new_state = CoroutineState::Ready;
                let old_state = self.change_state(new_state);
                self.on_ready(self, old_state);
                return Ok(());
            }
            _ => {}
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Ready
        )))
    }

    /// ready -> running
    /// syscall -> running
    ///
    /// below just for test
    /// suspend -> running
    ///
    /// # Errors
    /// if change state fails.
    pub fn running(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Running => return Ok(()),
            CoroutineState::Ready | CoroutineState::Syscall(_, _, SyscallState::Executing) => {
                let new_state = CoroutineState::Running;
                let old_state = self.change_state(new_state);
                self.on_running(self, old_state);
                return Ok(());
            }
            // #[cfg(test)] preemptive.rs use this
            CoroutineState::Suspend(_, timestamp) if timestamp <= now() => {
                let new_state = CoroutineState::Running;
                let old_state = self.change_state(new_state);
                self.on_running(self, old_state);
                return Ok(());
            }
            CoroutineState::Syscall(_, _, SyscallState::Callback | SyscallState::Timeout) => {
                return Ok(());
            }
            _ => {}
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Running
        )))
    }

    /// running -> suspend
    ///
    /// # Errors
    /// if change state fails.
    pub(super) fn suspend(&self, val: Yield, timestamp: u64) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            let new_state = CoroutineState::Suspend(val, timestamp);
            let old_state = self.change_state(new_state);
            self.on_suspend(self, old_state);
            return Ok(());
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Suspend(val, timestamp)
        )))
    }

    /// running -> syscall
    /// inner: syscall -> syscall
    ///
    /// # Errors
    /// if change state fails.
    pub fn syscall(
        &self,
        val: Yield,
        syscall: SyscallName,
        syscall_state: SyscallState,
    ) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Running => {
                let new_state = CoroutineState::Syscall(val, syscall, syscall_state);
                let old_state = self.change_state(new_state);
                self.on_syscall(self, old_state);
                return Ok(());
            }
            CoroutineState::Syscall(_, original_syscall, _) if original_syscall == syscall => {
                let new_state = CoroutineState::Syscall(val, syscall, syscall_state);
                let old_state = self.change_state(new_state);
                self.on_syscall(self, old_state);
                return Ok(());
            }
            _ => {}
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Syscall(val, syscall, syscall_state)
        )))
    }

    /// running -> cancel
    ///
    /// # Errors
    /// if change state fails.
    pub(super) fn cancel(&self) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            let new_state = CoroutineState::Cancelled;
            let old_state = self.change_state(new_state);
            self.on_cancel(self, old_state);
            return Ok(());
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Cancelled
        )))
    }

    /// running -> complete
    ///
    /// # Errors
    /// if change state fails.
    pub(super) fn complete(&self, val: Return) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            let new_state = CoroutineState::Complete(val);
            let old_state = self.change_state(new_state);
            self.on_complete(self, old_state, val);
            return Ok(());
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Complete(val)
        )))
    }

    /// running -> error
    ///
    /// # Errors
    /// if change state fails.
    pub(super) fn error(&self, msg: &'static str) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            let new_state = CoroutineState::Error(msg);
            let old_state = self.change_state(new_state);
            self.on_error(self, old_state, msg);
            return Ok(());
        }
        Err(Error::other(format!(
            "{} unexpected {current}->{:?}",
            self.name(),
            CoroutineState::<Yield, Return>::Error(msg)
        )))
    }
}

#[cfg(all(test, not(feature = "preemptive")))]
mod tests {
    use super::*;
    use crate::coroutine::suspender::Suspender;

    #[test]
    fn test_ready() -> std::io::Result<()> {
        let co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.ready()?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.suspend((), u64::MAX)?;
        assert_eq!(CoroutineState::Suspend((), u64::MAX), co.state());
        assert!(co.ready().is_err());
        Ok(())
    }

    #[test]
    fn test_running() -> std::io::Result<()> {
        let co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.running()?;
        co.complete(())?;
        assert_eq!(CoroutineState::Complete(()), co.state());
        assert!(co.running().is_err());
        Ok(())
    }

    #[test]
    fn test_suspend() -> std::io::Result<()> {
        let mut co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.suspend((), u64::MAX)?;
        assert_eq!(CoroutineState::Suspend((), u64::MAX), co.state());
        assert!(co.resume().is_err());
        assert!(co.suspend((), u64::MAX).is_err());
        Ok(())
    }

    #[test]
    fn test_syscall() -> std::io::Result<()> {
        let co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.syscall((), SyscallName::nanosleep, SyscallState::Executing)?;
        assert_eq!(
            CoroutineState::Syscall((), SyscallName::nanosleep, SyscallState::Executing),
            co.state()
        );
        assert!(co
            .syscall((), SyscallName::sleep, SyscallState::Executing)
            .is_err());
        Ok(())
    }

    #[test]
    fn test_complete() -> std::io::Result<()> {
        let co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.complete(())?;
        assert_eq!(CoroutineState::Complete(()), co.state());
        assert!(co.complete(()).is_err());
        Ok(())
    }

    #[test]
    fn test_error() -> std::io::Result<()> {
        let co = co!(|_: &Suspender<(), ()>, ()| {})?;
        assert_eq!(CoroutineState::Ready, co.state());
        co.running()?;
        co.error("test error, ignore it")?;
        assert_eq!(CoroutineState::Error("test error, ignore it"), co.state());
        assert!(co.error("abc").is_err());
        Ok(())
    }
}
