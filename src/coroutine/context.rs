use crate::coroutine::set_jmp::{longjmp, setjmp, JmpBuf};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::mem::MaybeUninit;
use std::rc::Rc;

pub struct Context<R> {
    f: Rc<Box<dyn FnOnce()>>,
    from: RefCell<JmpBuf>,
    point: RefCell<JmpBuf>,
    point_init: Cell<bool>,
    called: Cell<bool>,
    result: RefCell<MaybeUninit<R>>,
}

thread_local! {
    static CONTEXT: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl<R> Context<R> {
    pub fn new(f: impl FnOnce(&Context<R>) -> R + 'static) -> Self {
        Context {
            f: Rc::new(Box::new(move || {
                let context = Context::current().expect("should have a context");
                let r = f(context);
                let _ = context.result.replace(MaybeUninit::new(r));
                Context::<R>::clean_current();
                context.suspend();
            })),
            from: unsafe { std::mem::zeroed() },
            point: unsafe { std::mem::zeroed() },
            point_init: Cell::new(false),
            called: Cell::new(false),
            result: RefCell::new(MaybeUninit::uninit()),
        }
    }

    #[allow(clippy::pedantic)]
    fn init_current(coroutine: &Context<R>) {
        CONTEXT.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        });
    }

    pub(crate) fn current<'a>() -> Option<&'a Context<R>> {
        CONTEXT.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr.cast::<Context<R>>()) })
            }
        })
    }

    fn clean_current() {
        CONTEXT.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    pub fn get_result(&self) -> MaybeUninit<R> {
        self.result.replace(MaybeUninit::uninit())
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

impl<R> Debug for Context<R> {
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
        assert_eq!(unsafe { context.get_result().assume_init() }, 1);
    }
}
