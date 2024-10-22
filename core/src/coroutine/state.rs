use crate::common::constants::{CoroutineState, Syscall, SyscallState};
use crate::common::now;
use crate::coroutine::listener::Listener;
use crate::coroutine::Coroutine;
use crate::{error, info};
use std::fmt::Debug;
use std::io::{Error, ErrorKind};

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
        self.on_state_changed(self, old_state, new_state);
        if let CoroutineState::Error(_) = new_state {
            error!("{} {:?}->{:?}", self.name(), old_state, new_state);
        } else {
            info!("{} {:?}->{:?}", self.name(), old_state, new_state);
        }
        old_state
    }

    /// suspend -> ready
    ///
    /// # Errors
    /// if change state fails.
    pub(crate) fn ready(&self) -> std::io::Result<()> {
        let current = self.state();
        if let CoroutineState::Suspend(_, timestamp) = current {
            if timestamp <= now() {
                let new_state = CoroutineState::Ready;
                let old_state = self.change_state(new_state);
                self.on_ready(self, old_state);
                return Ok(());
            }
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::Ready
            ),
        ))
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
            CoroutineState::Ready | CoroutineState::SystemCall(_, _, SyscallState::Executing) => {
                let new_state = CoroutineState::Running;
                let old_state = self.change_state(new_state);
                self.on_running(self, old_state);
                return Ok(());
            }
            // #[cfg(test)] preemptive.rs use this
            CoroutineState::Suspend(_, timestamp) => {
                if timestamp <= now() {
                    let new_state = CoroutineState::Running;
                    let old_state = self.change_state(new_state);
                    self.on_running(self, old_state);
                    return Ok(());
                }
            }
            CoroutineState::SystemCall(_, _, SyscallState::Callback | SyscallState::Timeout) => {
                return Ok(());
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::Running
            ),
        ))
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
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::Suspend(val, timestamp)
            ),
        ))
    }

    /// running -> syscall
    /// inner: syscall -> syscall
    ///
    /// # Errors
    /// if change state fails.
    pub fn syscall(
        &self,
        val: Yield,
        syscall: Syscall,
        syscall_state: SyscallState,
    ) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Running => {
                let new_state = CoroutineState::SystemCall(val, syscall, syscall_state);
                let old_state = self.change_state(new_state);
                self.on_syscall(self, old_state);
                return Ok(());
            }
            CoroutineState::SystemCall(_, original_syscall, _) => {
                if original_syscall == syscall {
                    let new_state = CoroutineState::SystemCall(val, syscall, syscall_state);
                    let old_state = self.change_state(new_state);
                    self.on_syscall(self, old_state);
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::SystemCall(val, syscall, syscall_state)
            ),
        ))
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
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::Complete(val)
            ),
        ))
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
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.name(),
                CoroutineState::<Yield, Return>::Error(msg)
            ),
        ))
    }
}
