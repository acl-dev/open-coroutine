use backtrace::{Backtrace, BacktraceFrame};
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};

thread_local! {
    static BACKTRACES: Box<RefCell<Vec<BacktraceFrame>>> = Box::new(RefCell::new(Vec::new()));
}

pub(crate) fn add_backtrace() {
    BACKTRACES.with(|boxed| {
        let mut frames: Vec<BacktraceFrame> = Backtrace::new().into();
        let len = frames.len().saturating_sub(8);
        if len == 0 {
            return;
        }
        for _ in 0..len {
            let frame = frames.pop().unwrap();
            boxed.borrow_mut().push(frame);
        }
    });
}

fn history() -> Vec<BacktraceFrame> {
    BACKTRACES.with(|boxed| boxed.replace(Vec::new()))
}

pub struct StackTrace(Backtrace);

impl StackTrace {
    pub fn new() -> Self {
        let mut history = history();
        let mut frames: Vec<BacktraceFrame> = Backtrace::new().into();
        frames.retain(|frame| {
            for symbol in frame.symbols() {
                if let Some(name) = symbol.name() {
                    let res = format!("{name}");
                    if res.contains("open_coroutine_core::stack_trace::StackTrace::new") {
                        return false;
                    }
                }
            }
            true
        });
        if history.is_empty() {
            return StackTrace(Backtrace::from(frames));
        }
        while !history.is_empty() {
            frames.push(history.pop().unwrap());
        }
        StackTrace(Backtrace::from(frames))
    }
}

impl Debug for StackTrace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Default for StackTrace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OpenCoroutine, Scheduler, Yielder};
    use std::ffi::c_void;

    #[test]
    fn test_trace() {
        println!("{:?}", StackTrace::new());
    }

    #[test]
    fn test_coroutine_trace() {
        extern "C" fn context_func(
            _yielder: &Yielder<usize, usize, usize>,
            _input: usize,
        ) -> usize {
            println!("{:?}", StackTrace::new());
            1
        }
        let coroutine =
            OpenCoroutine::new(context_func, 0, 2048).expect("create coroutine failed !");
        assert_eq!(1, coroutine.resume_with(0).as_return().unwrap());
    }

    fn null() -> &'static mut c_void {
        unsafe { std::mem::transmute(10usize) }
    }

    #[test]
    fn test_scheduler_trace() {
        let mut scheduler = Scheduler::new();
        extern "C" fn f1(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine1] launched");
            null()
        }
        scheduler.submit(f1, null(), 4096).expect("submit failed !");
        extern "C" fn f2(
            _yielder: &Yielder<&'static mut c_void, (), &'static mut c_void>,
            _input: &'static mut c_void,
        ) -> &'static mut c_void {
            println!("[coroutine2] launched");
            println!("{:?}", StackTrace::new());
            null()
        }
        scheduler.submit(f2, null(), 4096).expect("submit failed !");
        scheduler.try_schedule().expect("try_schedule failed !");
    }
}
