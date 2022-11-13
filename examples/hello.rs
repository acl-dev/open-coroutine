use open_coroutine::OpenYielder;
use open_coroutine::co;
use std::os::raw::c_void;

fn main() {
    extern "C" fn f1(
        _yielder: &OpenYielder<Option<&'static mut c_void>>,
        _input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        println!("hello1 from coroutine");
        None
    }
    co(f1, None, 2048);
    extern "C" fn f2(
        _yielder: &OpenYielder<Option<&'static mut c_void>>,
        _input: Option<&'static mut c_void>,
    ) -> Option<&'static mut c_void> {
        println!("hello2 from coroutine");
        None
    }
    co(f2, None, 2048);
    unsafe {
        libc::sleep(1);
    }
}
