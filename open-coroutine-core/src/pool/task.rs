use crate::coroutine::suspender::SuspenderImpl;
use std::fmt::{Debug, Formatter};

#[repr(C)]
#[allow(clippy::type_complexity)]
pub struct Task<'t> {
    name: &'t str,
    func: Box<dyn FnOnce(&SuspenderImpl<(), ()>, ()) -> usize>,
}

impl<'t> Task<'t> {
    pub fn new(
        name: Box<str>,
        func: impl FnOnce(&SuspenderImpl<'_, (), ()>, ()) -> usize + 'static,
    ) -> Self {
        Task {
            name: Box::leak(name),
            func: Box::new(func),
        }
    }

    #[must_use]
    pub fn get_name(&self) -> &'t str {
        self.name
    }

    #[must_use]
    pub fn run(self, suspender: &SuspenderImpl<'_, (), ()>) -> usize {
        (self.func)(suspender, ())
    }
}

impl Debug for Task<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task").field("name", &self.name).finish()
    }
}
