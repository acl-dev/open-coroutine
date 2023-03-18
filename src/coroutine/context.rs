use crate::coroutine::set_jmp::{longjmp, setjmp, JmpBuf};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

pub struct Context {
    f: Rc<Box<dyn FnOnce()>>,
    from: RefCell<JmpBuf>,
    point: RefCell<JmpBuf>,
    point_init: Cell<bool>,
    called: Cell<bool>,
}

thread_local! {
    static CONTEXT: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl Context {
    pub fn new<R>(f: impl FnOnce(&Context) -> R + 'static) -> Self {
        Context {
            f: Rc::new(Box::new(move || {
                let context = Context::current().expect("should have a context");
                let _ = f(context);
                Context::clean_current();
                context.suspend();
            })),
            from: unsafe { std::mem::zeroed() },
            point: unsafe { std::mem::zeroed() },
            point_init: Cell::new(false),
            called: Cell::new(false),
        }
    }

    #[allow(clippy::pedantic)]
    fn init_current(coroutine: &Context) {
        CONTEXT.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        });
    }

    pub(crate) fn current<'a>() -> Option<&'a Context> {
        CONTEXT.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr.cast::<Context>()) })
            }
        })
    }

    fn clean_current() {
        CONTEXT.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    pub fn resume(&self) {
        unsafe {
            if setjmp(self.from.as_ptr()) == 0 {
                if self.point_init.get() {
                    longjmp(self.point.as_ptr(), 1);
                    unreachable!();
                } else if !self.called.get() {
                    self.called.set(true);
                    Context::init_current(self);
                    (std::ptr::read_unaligned(self.f.as_ref()))();
                }
            }
        }
    }

    pub fn suspend(&self) {
        unsafe {
            if setjmp(self.point.as_ptr()) == 0 {
                self.point_init.set(true);
                longjmp(self.from.as_ptr(), 1);
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
        let context = Context::new(move |c| {
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
