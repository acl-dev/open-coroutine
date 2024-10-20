use crate::catch;
use derivative::Derivative;

/// 做C兼容时会用到
pub type UserTaskFunc = extern "C" fn(usize) -> usize;

/// The task impls.
#[repr(C)]
#[derive(Derivative)]
#[derivative(Debug)]
pub struct Task<'t> {
    name: String,
    #[derivative(Debug = "ignore")]
    func: Box<dyn FnOnce(Option<usize>) -> Option<usize> + 't>,
    param: Option<usize>,
}

impl<'t> Task<'t> {
    /// Create a new `Task` instance.
    pub fn new(
        name: String,
        func: impl FnOnce(Option<usize>) -> Option<usize> + 't,
        param: Option<usize>,
    ) -> Self {
        Task {
            name,
            func: Box::new(func),
            param,
        }
    }

    /// execute the task
    ///
    /// # Errors
    /// if an exception occurred while executing this task.
    pub fn run<'e>(self) -> (String, Result<Option<usize>, &'e str>) {
        (
            self.name.clone(),
            catch!(
                || (self.func)(self.param),
                format!("task {} failed without message", self.name),
                format!("task {}", self.name)
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::co_pool::task::Task;

    #[test]
    fn test() {
        let task = Task::new(
            String::from("test"),
            |p| {
                println!("hello");
                p
            },
            None,
        );
        assert_eq!((String::from("test"), Ok(None)), task.run());
    }

    #[test]
    fn test_panic() {
        let task = Task::new(
            String::from("test"),
            |_| {
                panic!("test panic, just ignore it");
            },
            None,
        );
        assert_eq!(
            (String::from("test"), Err("test panic, just ignore it")),
            task.run()
        );
    }
}
