use crate::coroutine::set_jmp::{longjmp, setjmp, JmpBuf};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::rc::Rc;

/// The result of a generator resumption.
///
/// This is modeled after the stdlib's nightly-only
/// [`std::ops::GeneratorState`].
#[derive(PartialEq, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum GeneratorState<R> {
    /// The generator suspended.
    Yielded,

    /// The generator completed with a return value.
    Complete(R),
}

pub struct Context<'c, R> {
    f: Rc<Box<dyn FnOnce()>>,
    from: RefCell<JmpBuf>,
    point: RefCell<JmpBuf>,
    started: Cell<bool>,
    finished: Cell<bool>,
    result: RefCell<MaybeUninit<R>>,

    // We want to be covariant over 'c, Yield and Return, and contravariant
    // over Input.
    //
    // Effectively this means that we can pass a
    //   ScopedCoroutine<'static, &'a (), &'static (), &'static ()>
    // to a function that expects a
    //   ScopedCoroutine<'b, &'static (), &'c (), &'d ()>
    marker: PhantomData<&'c fn() -> GeneratorState<R>>,
}

thread_local! {
    static CONTEXT: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

impl<'c, R> Context<'c, R> {
    pub fn new(f: impl FnOnce(&Context<R>) -> R + 'static) -> Self {
        Context {
            f: Rc::new(Box::new(move || {
                let context = Context::current().expect("should have a context");
                let r = f(context);
                let _ = context.result.replace(MaybeUninit::new(r));
                assert!(!context.finished.replace(true));
                Context::<R>::clean_current();
                context.suspend();
            })),
            from: unsafe { std::mem::zeroed() },
            point: unsafe { std::mem::zeroed() },
            started: Cell::new(false),
            finished: Cell::new(false),
            result: RefCell::new(MaybeUninit::uninit()),
            marker: PhantomData,
        }
    }

    #[allow(clippy::pedantic)]
    fn init_current(coroutine: &Context<R>) {
        CONTEXT.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        });
    }

    pub(crate) fn current<'a>() -> Option<&'a Context<'c, R>> {
        CONTEXT.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr.cast::<Context<'c, R>>()) })
            }
        })
    }

    fn clean_current() {
        CONTEXT.with(|boxed| *boxed.borrow_mut() = std::ptr::null());
    }

    pub fn is_finished(&self) -> bool {
        self.finished.get()
    }

    pub fn resume(&self) -> GeneratorState<R> {
        unsafe {
            if setjmp(self.from.as_ptr()) == 0 {
                if self.started.get() {
                    longjmp(self.point.as_ptr(), 1);
                } else {
                    self.started.set(true);
                    Context::init_current(self);
                    (std::ptr::read_unaligned(self.f.as_ref()))();
                }
                unreachable!();
            }
            if self.is_finished() {
                let r = self.result.replace(MaybeUninit::uninit()).assume_init();
                GeneratorState::Complete(r)
            } else {
                GeneratorState::Yielded
            }
        }
    }

    pub fn suspend(&self) {
        unsafe {
            if setjmp(self.point.as_ptr()) == 0 {
                longjmp(self.from.as_ptr(), 1);
                unreachable!();
            }
        }
    }
}

impl<R> Debug for Context<'_, R> {
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
        assert_eq!(GeneratorState::Yielded, context.resume());
        assert_eq!(GeneratorState::Yielded, context.resume());
        assert_eq!(GeneratorState::Complete(1), context.resume());
    }
}
