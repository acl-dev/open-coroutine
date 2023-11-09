/// Coroutine local abstraction.
pub mod local;

pub mod suspender;

#[macro_export]
macro_rules! co {
    ($f:expr, $size:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            $size,
        )
        .expect("create coroutine failed !")
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr, $size:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(Box::from($name), $f, $size)
            .expect("create coroutine failed !")
    };
    ($name:literal, $f:expr $(,)?) => {
        $crate::coroutine::CoroutineImpl::new(
            Box::from($name),
            $f,
            $crate::constants::DEFAULT_STACK_SIZE,
        )
        .expect("create coroutine failed !")
    };
}

#[cfg(feature = "korosensei")]
pub use korosensei::CoroutineImpl;
#[allow(missing_docs)]
#[cfg(feature = "korosensei")]
mod korosensei;

#[cfg(test)]
mod tests;
