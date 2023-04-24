use crate::join::JoinHandle;
use open_coroutine_core::coroutine::suspender::Suspender;
use open_coroutine_core::event_loop::UserFunc;
use std::ffi::c_void;

#[allow(improper_ctypes)]
extern "C" {
    fn coroutine_crate(f: UserFunc, param: &'static mut c_void, stack_size: usize) -> JoinHandle;
}

#[allow(dead_code)]
fn co<F, P: 'static, R: 'static>(f: F, param: P, stack_size: usize) -> JoinHandle
where
    F: FnOnce(*const Suspender<(), ()>, P) -> R + Copy,
{
    extern "C" fn co_main<F, P: 'static, R: 'static>(
        suspender: *const Suspender<(), ()>,
        input: &'static mut c_void,
    ) -> &'static mut c_void
    where
        F: FnOnce(*const Suspender<(), ()>, P) -> R + Copy,
    {
        unsafe {
            let ptr = &mut *((input as *mut c_void).cast::<(F, P)>());
            let data = std::ptr::read_unaligned(ptr);
            let result: &'static mut R = Box::leak(Box::new((data.0)(suspender, data.1)));
            &mut *((result as *mut R).cast::<c_void>())
        }
    }
    let inner = Box::leak(Box::new((f, param)));
    unsafe {
        coroutine_crate(
            co_main::<F, P, R>,
            &mut *((inner as *mut (F, P)).cast::<c_void>()),
            stack_size,
        )
    }
}

#[macro_export]
macro_rules! co {
    ( $f: expr , $param:expr $(,)? ) => {{
        $crate::coroutine::co(
            $f,
            $param,
            open_coroutine_core::coroutine::default_stack_size(),
        )
    }};
    ( $f: expr , $param:expr ,$stack_size: expr $(,)?) => {{
        $crate::coroutine::co($f, $param, $stack_size)
    }};
}

#[cfg(test)]
mod tests {

    #[test]
    fn simplest() {
        let handler1 = co!(
            |_, input| {
                println!("[coroutine1] launched with {}", input);
                input
            },
            1,
            4096,
        );
        let handler2 = co!(
            |_, input| {
                println!("[coroutine2] launched with {}", input);
                input
            },
            "hello",
        );
        unsafe {
            assert_eq!(1, handler1.join().unwrap().unwrap());
            assert_eq!("hello", &*handler2.join::<*mut str>().unwrap().unwrap());
        }
    }
}
