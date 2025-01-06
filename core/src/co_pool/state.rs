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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state() -> std::io::Result<()> {
        let mut pool = CoroutinePool::default();
        _ = pool.stopping()?;
        assert_eq!(PoolState::Stopping, pool.state());
        _ = pool.stopping()?;
        assert_eq!(PoolState::Stopping, pool.state());
        _ = pool.stopped()?;
        assert!(pool.stopped().is_ok());
        assert!(pool.stopping().is_err());
        assert!(pool.try_schedule_task().is_err());
        Ok(())
    }
}
