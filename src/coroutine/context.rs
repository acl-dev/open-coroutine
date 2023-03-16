use crate::set_jump::{longjmp, setjmp, JmpBuf};
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

pub struct Context {
    f: Rc<Box<dyn FnOnce(&mut Context)>>,
    from: JmpBuf,
    point: JmpBuf,
    point_init: bool,
    called: bool,
}

impl Context {
    pub fn new(f: impl FnOnce(&mut Context) + 'static) -> Self {
        unsafe {
            Context {
                f: Rc::new(Box::new(f)),
                from: std::mem::zeroed(),
                point: std::mem::zeroed(),
                point_init: false,
                called: false,
            }
        }
    }

    pub fn resume(&mut self) {
        unsafe {
            if setjmp(&mut self.from) == 0 {
                if self.point_init {
                    longjmp(&mut self.point, 1);
                    unreachable!();
                } else if !self.called {
                    self.called = true;
                    (std::ptr::read_unaligned(self.f.as_ref()))(self);
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
        let user_func = move |s: &mut Context| {
            println!("{}", str);
            s.suspend();
            println!("second time through");
            s.suspend();
            println!("third time through");
            1
        };

        let mut suspender = Context::new(move |s| {
            let _ = user_func(s);
            s.suspend();
        });
        suspender.resume();
        suspender.resume();
        suspender.resume();
    }
}
