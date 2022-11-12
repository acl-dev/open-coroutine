use base_coroutine::ContextFn;
use std::os::raw::c_void;

#[allow(dead_code)]
extern "C" {
    fn coroutine_crate(
        f: ContextFn<Option<&'static mut c_void>, Option<&'static mut c_void>>,
        param: Option<&'static mut c_void>,
        stack_size: usize,
    );
}

#[cfg(test)]
mod tests {
    use crate::coroutine_crate;
    use base_coroutine::OpenYielder;
    use std::os::raw::c_void;

    #[test]
    fn test_sleep() {
        unsafe {
            extern "C" fn f1(
                _yielder: &OpenYielder<Option<&'static mut c_void>>,
                _input: Option<&'static mut c_void>,
            ) -> Option<&'static mut c_void> {
                println!("hello1 from coroutine");
                None
            }
            coroutine_crate(f1, None, 2048);
            extern "C" fn f2(
                _yielder: &OpenYielder<Option<&'static mut c_void>>,
                _input: Option<&'static mut c_void>,
            ) -> Option<&'static mut c_void> {
                println!("hello2 from coroutine");
                None
            }
            coroutine_crate(f2, None, 2048);
            libc::sleep(1);
        }
    }
}
