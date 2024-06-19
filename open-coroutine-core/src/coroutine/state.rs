use crate::common::Named;
use crate::constants::{CoroutineState, Syscall, SyscallState};
use crate::coroutine::CoroutineImpl;
use crate::info;
use std::fmt::Debug;
use std::io::{Error, ErrorKind};
use std::panic::UnwindSafe;

impl<'c, Param, Yield, Return> CoroutineImpl<'c, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: Copy + Eq + PartialEq + UnwindSafe + Debug,
    Return: Copy + Eq + PartialEq + UnwindSafe + Debug,
{
    /// Returns the previous state of this `StateCoroutine`.
    /// Note: user should not use this method.
    fn change_state(&self, state: CoroutineState<Yield, Return>) -> CoroutineState<Yield, Return> {
        self.state.replace(state)
    }

    /// created -> ready
    /// suspend -> ready
    ///
    /// # Errors
    /// if change state fails.
    pub fn ready(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Created => {
                _ = self.change_state(CoroutineState::Ready);
                return Ok(());
            }
            CoroutineState::Suspend(_, timestamp) => {
                if timestamp <= open_coroutine_timer::now() {
                    _ = self.change_state(CoroutineState::Ready);
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Yield, Return>::Ready
            ),
        ))
    }

    /// ready -> running
    /// syscall -> running
    ///
    /// below just for test
    /// created -> running
    /// suspend -> running
    ///
    /// # Errors
    /// if change state fails.
    pub fn running(&self) -> std::io::Result<()> {
        let current = self.state();
        match current {
            CoroutineState::Running => return Ok(()),
            #[cfg(test)]
            CoroutineState::Created => {
                _ = self.change_state(CoroutineState::Running);
                return Ok(());
            }
            CoroutineState::Ready => {
                _ = self.change_state(CoroutineState::Running);
                return Ok(());
            }
            // #[cfg(test)] preemptive.rs use this
            CoroutineState::Suspend(_, timestamp) => {
                if timestamp <= open_coroutine_timer::now() {
                    _ = self.change_state(CoroutineState::Running);
                    return Ok(());
                }
            }
            CoroutineState::SystemCall(
                _,
                _,
                SyscallState::Executing | SyscallState::Finished | SyscallState::Timeout,
            ) => {
                let state = CoroutineState::Running;
                _ = self.change_state(state);
                info!("{} {:?}->{:?}", self.get_name(), current, state);
                return Ok(());
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Yield, Return>::Running
            ),
        ))
    }

    /// running -> suspend
    ///
    /// # Errors
    /// if change state fails.
    pub fn suspend(&self, val: Yield, timestamp: u64) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Suspend(val, timestamp));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
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
                let state = CoroutineState::SystemCall(val, syscall, syscall_state);
                _ = self.change_state(state);
                info!("{} {:?}->{:?}", self.get_name(), current, state);
                return Ok(());
            }
            CoroutineState::SystemCall(_, original_syscall, _) => {
                if original_syscall == syscall {
                    let state = CoroutineState::SystemCall(val, syscall, syscall_state);
                    _ = self.change_state(state);
                    info!("{} {:?}->{:?}", self.get_name(), current, state);
                    return Ok(());
                }
            }
            _ => {}
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Yield, Return>::SystemCall(val, syscall, syscall_state)
            ),
        ))
    }

    /// running -> complete
    ///
    /// # Errors
    /// if change state fails.
    pub fn complete(&self, val: Return) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Complete(val));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Yield, Return>::Complete(val)
            ),
        ))
    }

    /// running -> error
    ///
    /// # Errors
    /// if change state fails.
    pub fn error(&self, val: &'static str) -> std::io::Result<()> {
        let current = self.state();
        if CoroutineState::Running == current {
            _ = self.change_state(CoroutineState::Error(val));
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Other,
            format!(
                "{} unexpected {current}->{:?}",
                self.get_name(),
                CoroutineState::<Yield, Return>::Error(val)
            ),
        ))
    }
}
