use base_coroutine::ContextFn;
use std::os::raw::c_void;

#[allow(dead_code)]
extern "C" {
    fn init_hook();

    fn coroutine_crate(
        f: ContextFn<Option<&'static mut c_void>, Option<&'static mut c_void>>,
        param: Option<&'static mut c_void>,
        stack_size: usize,
    );
}

pub fn init() {
    unsafe { init_hook() }
}

pub fn co(
    f: ContextFn<Option<&'static mut c_void>, Option<&'static mut c_void>>,
    param: Option<&'static mut c_void>,
    stack_size: usize,
) {
    unsafe { coroutine_crate(f, param, stack_size) }
}
