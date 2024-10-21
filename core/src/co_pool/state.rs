use crate::co_pool::CoroutinePool;
use crate::common::constants::PoolState;
use std::io::{Error, ErrorKind};

impl CoroutinePool<'_> {
    /// running -> stopping
    ///
    /// # Errors
    /// if change state fails.
    pub(crate) fn stopping(&self) -> std::io::Result<PoolState> {
        self.change_state(PoolState::Running, PoolState::Stopping)
    }

    /// stopping -> stopped
    ///
    /// # Errors
    /// if change state fails.
    pub(crate) fn stopped(&self) -> std::io::Result<PoolState> {
        self.change_state(PoolState::Stopping, PoolState::Stopped)
    }

    /// Get the state of this coroutine.
    pub fn state(&self) -> PoolState {
        self.state.get()
    }

    fn change_state(
        &self,
        old_state: PoolState,
        new_state: PoolState,
    ) -> std::io::Result<PoolState> {
        let current = self.state();
        if current == new_state {
            return Ok(old_state);
        }
        if current == old_state {
            assert_eq!(old_state, self.state.replace(new_state));
            return Ok(old_state);
        }
        Err(Error::new(
            ErrorKind::Other,
            format!("{} unexpected {current}->{:?}", self.name(), new_state),
        ))
    }
}
