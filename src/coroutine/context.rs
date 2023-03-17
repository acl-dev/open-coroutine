use crate::coroutine::set_jmp::{longjmp, setjmp, JmpBuf};
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

pub struct Context {
    f: Rc<Box<dyn FnOnce()>>,
    from: JmpBuf,
    point: JmpBuf,
    point_init: bool,
    called: bool,
}

thread_local! {
    static CONTEXT: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

impl Context {
    pub fn new<R>(f: impl FnOnce(&mut Context) -> R + 'static) -> Self {
        Context {
            f: Rc::new(Box::new(move || {
                let context = Context::current().expect("should have a context");
                let _ = f(context);
                Context::clean_current();
                context.suspend();
            })),
            from: unsafe { std::mem::zeroed() },
            point: unsafe { std::mem::zeroed() },
            point_init: false,
            called: false,
        }
    }

    #[allow(clippy::pedantic)]
    fn init_current(coroutine: &mut Context) {
        CONTEXT.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *mut _ as *mut c_void;
        });
    }

    pub(crate) fn current<'a>() -> Option<&'a mut Context> {
        CONTEXT.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &mut *(ptr.cast::<Context>()) })
            }
        })
    }

    fn clean_current() {
        CONTEXT.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut());
    }

    pub fn resume(&mut self) {
        unsafe {
            if setjmp(&mut self.from) == 0 {
                if self.point_init {
                    longjmp(&mut self.point, 1);
                    unreachable!();
                } else if !self.called {
                    self.called = true;
                    Context::init_current(self);
                    (std::ptr::read_unaligned(self.f.as_ref()))();
                }
            }
        }
    }

    pub fn suspend(&mut self) {
        unsafe {
            if setjmp(&mut self.point) == 0 {
                self.point_init = true;
                longjmp(&mut self.from, 1);
                unreachable!();
            }
        }
    }
}

impl Debug for Context {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suspender")
            .field("from", &self.from)
            .field("to", &self.point)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let str = "first time through";
        let mut context = Context::new(move |c| {
            println!("{}", str);
            c.suspend();
            println!("second time through");
            c.suspend();
            println!("third time through");
            1
        });
        context.resume();
        context.resume();
        context.resume();
    }
}
