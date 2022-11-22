use base_coroutine::coroutine::UserFunc;
use base_coroutine::scheduler::Scheduler;
use std::os::raw::c_void;
use std::time::Duration;

/**
被hook的系统函数
#[no_mangle]避免rust编译器修改方法名称
 */
#[no_mangle]
pub extern "C" fn init_hook() {
    //啥都不做，只是为了保证hook的函数能够被重定向到
    //主要为了防止有的程序压根不调用coroutine_crate的情况
}

///创建协程
#[no_mangle]
pub extern "C" fn coroutine_crate(
    f: UserFunc<&'static mut c_void, (), &'static mut c_void>,
    param: &'static mut c_void,
    stack_size: usize,
) {
    Scheduler::current().submit(f, param, stack_size)
}

///轮询协程
#[no_mangle]
pub extern "C" fn try_timed_schedule(ns_time: u64) {
    Scheduler::current().try_timed_schedule(Duration::from_nanos(ns_time));
}

#[no_mangle]
pub extern "C" fn timed_schedule(ns_time: u64) {
    Scheduler::current().timed_schedule(Duration::from_nanos(ns_time));
}
