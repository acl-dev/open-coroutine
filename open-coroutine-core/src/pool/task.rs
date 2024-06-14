use crate::catch;
use crate::common::Named;
use crate::coroutine::suspender::Suspender;
use std::cell::Cell;
use std::fmt::{Debug, Formatter};
use std::panic::UnwindSafe;

/// Note: the param and the result is raw pointer.
#[repr(C)]
#[allow(clippy::type_complexity, box_pointers)]
pub struct Task<'t> {
    name: String,
    func: Box<
        dyn FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 't,
    >,
    param: Cell<Option<usize>>,
}

impl UnwindSafe for Task<'_> {}

impl Debug for Task<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("name", &self.name)
            .field("param", &self.param)
            .finish_non_exhaustive()
    }
}

impl Named for Task<'_> {
    fn get_name(&self) -> &str {
        &self.name
    }
}

#[allow(dead_code)]
impl<'t> Task<'t> {
    pub fn new(
        name: String,
        func: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, Option<usize>) -> Option<usize>
            + UnwindSafe
            + 't,
        param: Option<usize>,
    ) -> Self {
        Task {
            name,
            func: Box::new(func),
            param: Cell::new(param),
        }
    }

    /// Set a param for this task.
    pub fn set_param(&self, param: usize) -> Option<usize> {
        self.param.replace(Some(param))
    }

    /// Get param from this task.
    pub fn get_param(&self) -> Option<usize> {
        self.param.get()
    }

    /// exec the task
    ///
    /// # Errors
    /// if an exception occurred while executing this task.
    #[allow(box_pointers)]
    pub fn run<'e>(
        self,
        suspender: &dyn Suspender<Resume = (), Yield = ()>,
    ) -> (String, Result<Option<usize>, &'e str>) {
        let paran = self.get_param();
        (
            self.name.clone(),
            catch!(
                || (self.func)(suspender, paran),
                "task failed without message",
                "task",
                self.name
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::co;
    use crate::constants::CoroutineState;
    use crate::coroutine::{Coroutine, SimpleCoroutine};

    #[test]
    fn test() -> std::io::Result<()> {
        let mut coroutine = co!(|suspender, _| {
            let task = Task::new(
                String::from("test"),
                |_, p| {
                    println!("hello");
                    p
                },
                None,
            );
            assert_eq!((String::from("test"), Ok(None)), task.run(suspender));
        });
        assert_eq!(CoroutineState::Complete(()), coroutine.resume()?);
        Ok(())
    }

    #[test]
    fn test_panic() -> std::io::Result<()> {
        let mut coroutine = co!(|suspender, _| {
            let task = Task::new(
                String::from("test"),
                |_, _| panic!("test panic, just ignore it"),
                None,
            );
            assert_eq!(
                (String::from("test"), Err("test panic, just ignore it")),
                task.run(suspender)
            );
        });
        assert_eq!(CoroutineState::Complete(()), coroutine.resume()?);
        Ok(())
    }
}
