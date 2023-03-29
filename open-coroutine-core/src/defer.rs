struct Defer<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        self.0.take().map(|f| f());
    }
}

/// Defer execution of a closure until the return value is dropped.
pub fn defer<F: FnOnce()>(f: F) -> impl Drop {
    Defer(Some(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn test() {
        let i = RefCell::new(0);
        {
            let _d = defer(|| *i.borrow_mut() += 1);
            assert_eq!(*i.borrow(), 0);
        }
        assert_eq!(*i.borrow(), 1);
    }
}
