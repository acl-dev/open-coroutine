use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::ManuallyDrop;

thread_local! {
    static RESULT: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
}

pub(crate) fn init_result<R>(result: R) {
    RESULT.with(|boxed| {
        let mut r = ManuallyDrop::new(result);
        *boxed.borrow_mut() = &mut r as *mut _ as *mut c_void;
    })
}

pub(crate) fn take_result<R>() -> Option<R> {
    RESULT.with(|boxed| {
        let ptr = *boxed.borrow_mut();
        if ptr.is_null() {
            None
        } else {
            unsafe {
                let r = Some(ManuallyDrop::take(&mut *(ptr as *mut ManuallyDrop<R>)));
                *boxed.borrow_mut() = std::ptr::null_mut();
                r
            }
        }
    })
}
