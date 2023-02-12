use crate::scheduler::Scheduler;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt::{Debug, Formatter};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

use crate::monitor::Monitor;
pub use corosensei::stack::*;
pub use corosensei::*;

pub fn page_size() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);
    if ret == 0 {
        unsafe {
            cfg_if::cfg_if! {
                if #[cfg(windows)] {
                    let mut info = std::mem::zeroed();
                    windows_sys::Win32::System::SystemInformation::GetSystemInfo(&mut info);
                    ret = info.dwPageSize as usize
                } else {
                    ret = libc::sysconf(libc::_SC_PAGESIZE) as usize;
                }
            }
        }
        PAGE_SIZE.store(ret, Ordering::Relaxed);
    }
    ret
}

pub type UserFunc<'a, Param, Yield, Return> =
    extern "C" fn(&'a OpenYielder<Param, Yield>, Param) -> Return;

thread_local! {
    static DELAY_TIME: Box<RefCell<u64>> = Box::new(RefCell::new(0));
    static YIELDER: Box<RefCell<*mut c_void>> = Box::new(RefCell::new(std::ptr::null_mut()));
    static SYSCALL_FLAG: Box<RefCell<bool>> = Box::new(RefCell::new(false));
}

#[repr(transparent)]
pub struct OpenYielder<'a, Param, Yield>(&'a Yielder<Param, Yield>);

impl<'a, Param, Yield> OpenYielder<'a, Param, Yield> {
    pub(crate) fn new(yielder: &'a Yielder<Param, Yield>) -> Self {
        OpenYielder(yielder)
    }

    /// Suspends the execution of a currently running coroutine.
    ///
    /// This function will switch control back to the original caller of
    /// [`Coroutine::resume`]. This function will then return once the
    /// [`Coroutine::resume`] function is called again.
    pub extern "C" fn suspend(&self, val: Yield) -> Param {
        let yielder = OpenYielder::<Param, Yield>::yielder();
        OpenYielder::<Param, Yield>::clean_yielder();
        let param = self.0.suspend(val);
        unsafe { OpenYielder::init_yielder(&mut *yielder) };
        param
    }

    pub extern "C" fn delay(&self, val: Yield, ms_time: u64) -> Param {
        self.delay_ns(
            val,
            match ms_time.checked_mul(1_000_000) {
                Some(v) => v,
                None => u64::MAX,
            },
        )
    }

    pub extern "C" fn delay_ns(&self, val: Yield, ns_time: u64) -> Param {
        OpenYielder::<Param, Yield>::init_delay_time(ns_time);
        self.suspend(val)
    }

    fn init_yielder(yielder: &mut OpenYielder<Param, Yield>) {
        YIELDER.with(|boxed| {
            *boxed.borrow_mut() = yielder as *mut _ as *mut c_void;
        });
    }

    pub fn yielder<'y>() -> *mut OpenYielder<'y, Param, Yield> {
        YIELDER.with(|boxed| unsafe { std::mem::transmute(*boxed.borrow_mut()) })
    }

    fn clean_yielder() {
        YIELDER.with(|boxed| *boxed.borrow_mut() = std::ptr::null_mut())
    }

    fn init_delay_time(time: u64) {
        DELAY_TIME.with(|boxed| {
            *boxed.borrow_mut() = time;
        });
    }

    pub(crate) fn delay_time() -> u64 {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_delay() {
        DELAY_TIME.with(|boxed| *boxed.borrow_mut() = 0)
    }

    pub(crate) extern "C" fn syscall(&self, val: Yield) -> Param {
        OpenYielder::<Param, Yield>::init_syscall_flag();
        self.suspend(val)
    }

    fn init_syscall_flag() {
        SYSCALL_FLAG.with(|boxed| {
            *boxed.borrow_mut() = true;
        });
    }

    pub(crate) fn syscall_flag() -> bool {
        SYSCALL_FLAG.with(|boxed| *boxed.borrow_mut())
    }

    pub(crate) fn clean_syscall_flag() {
        SYSCALL_FLAG.with(|boxed| *boxed.borrow_mut() = false)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Status {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起
    Suspend,
    ///执行系统调用
    SystemCall,
    ///栈扩/缩容时
    CopyStack,
    ///调用用户函数完成，但未退出
    Finished,
    ///已退出
    Exited,
}

thread_local! {
    static COROUTINE: Box<RefCell<*const c_void>> = Box::new(RefCell::new(std::ptr::null()));
}

#[repr(C)]
pub struct OpenCoroutine<'a, Param, Yield, Return> {
    name: &'a str,
    sp: RefCell<ScopedCoroutine<'a, Param, Yield, Return, DefaultStack>>,
    status: Cell<Status>,
    //调用用户函数的参数
    param: RefCell<Param>,
    result: RefCell<MaybeUninit<ManuallyDrop<Return>>>,
    scheduler: RefCell<Option<*mut Scheduler>>,
}

unsafe impl<Param, Yield, Return> Send for OpenCoroutine<'_, Param, Yield, Return> {}
unsafe impl<Param, Yield, Return> Sync for OpenCoroutine<'_, Param, Yield, Return> {}

impl<'a, Param, Yield, Return> Drop for OpenCoroutine<'a, Param, Yield, Return> {
    fn drop(&mut self) {
        self.status.set(Status::Exited);
    }
}

impl<'a, Param, Yield, Return> OpenCoroutine<'a, Param, Yield, Return> {
    pub fn new<F>(f: F, param: Param, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&OpenYielder<Param, Yield>, Param) -> Return,
        F: 'a,
    {
        OpenCoroutine::with_name(&Uuid::new_v4().to_string(), f, param, size)
    }

    pub fn with_name<F>(name: &str, f: F, param: Param, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&OpenYielder<Param, Yield>, Param) -> Return,
        F: 'a,
    {
        let stack = DefaultStack::new(size)?;
        let sp = ScopedCoroutine::with_stack(stack, |yielder, param| {
            let mut open_yielder = OpenYielder::new(yielder);
            OpenYielder::<Param, Yield>::init_yielder(&mut open_yielder);
            OpenCoroutine::<Param, Yield, Return>::current()
                .unwrap()
                .set_status(Status::Running);
            f(&open_yielder, param)
        });
        Ok(OpenCoroutine {
            name: Box::leak(Box::from(name)),
            sp: RefCell::new(sp),
            status: Cell::new(Status::Created),
            param: RefCell::new(param),
            result: RefCell::new(MaybeUninit::uninit()),
            scheduler: RefCell::new(None),
        })
    }

    pub fn resume_with(&self, val: Param) -> (Param, CoroutineResult<Yield, Return>) {
        let previous = self.param.replace(val);
        (previous, self.resume())
    }

    pub fn resume(&self) -> CoroutineResult<Yield, Return> {
        self.set_status(Status::Ready);
        OpenCoroutine::init_current(self);
        let param = unsafe { std::ptr::read_unaligned(self.param.as_ptr()) };
        match self.sp.borrow_mut().resume(param) {
            CoroutineResult::Return(r) => {
                self.set_status(Status::Finished);
                OpenCoroutine::<Param, Yield, Return>::clean_current();
                OpenYielder::<Param, Yield>::clean_yielder();
                //还没执行到10ms就返回了，此时需要清理signal
                //否则下一个协程执行不到10ms就被抢占调度了
                Monitor::clean_task(Monitor::signal_time());
                if let Some(scheduler) = self.get_scheduler() {
                    self.result.replace(MaybeUninit::new(ManuallyDrop::new(r)));
                    Scheduler::save_result(unsafe {
                        std::ptr::read_unaligned(std::mem::transmute(self))
                    });
                    //执行下一个用户协程
                    unsafe { (*scheduler).do_schedule() };
                    unreachable!("should not execute to here !")
                } else {
                    CoroutineResult::Return(r)
                }
            }
            CoroutineResult::Yield(y) => {
                self.set_status(Status::Suspend);
                OpenCoroutine::<Param, Yield, Return>::clean_current();
                //还没执行到10ms就主动yield了，此时需要清理signal
                //否则下一个协程执行不到10ms就被抢占调度了
                Monitor::clean_task(Monitor::signal_time());
                CoroutineResult::Yield(y)
            }
        }
    }

    fn init_current(coroutine: &OpenCoroutine<'a, Param, Yield, Return>) {
        COROUTINE.with(|boxed| {
            *boxed.borrow_mut() = coroutine as *const _ as *const c_void;
        })
    }

    pub fn current() -> Option<&'a OpenCoroutine<'a, Param, Yield, Return>> {
        COROUTINE.with(|boxed| {
            let ptr = *boxed.borrow_mut();
            if ptr.is_null() {
                None
            } else {
                Some(unsafe { &*(ptr as *const OpenCoroutine<'a, Param, Yield, Return>) })
            }
        })
    }

    fn clean_current() {
        COROUTINE.with(|boxed| *boxed.borrow_mut() = std::ptr::null())
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn get_status(&self) -> Status {
        self.status.get()
    }

    pub fn set_status(&self, status: Status) {
        self.status.set(status);
    }

    pub fn is_finished(&self) -> bool {
        self.get_status() == Status::Finished
    }

    pub fn get_result(&self) -> Option<Return> {
        if self.is_finished() {
            unsafe {
                let mut m = self.result.borrow().assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            }
        } else {
            None
        }
    }

    pub fn get_scheduler(&self) -> Option<*mut Scheduler> {
        *self.scheduler.borrow()
    }

    pub(crate) fn set_scheduler(&self, scheduler: &mut Scheduler) {
        self.scheduler.replace(Some(scheduler));
    }
}

impl<'a, Param, Yield, Return> Debug for OpenCoroutine<'a, Param, Yield, Return> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCoroutine")
            .field("name", &self.name)
            .field("status", &self.status)
            .field("scheduler", &self.scheduler)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return() {
        let coroutine = OpenCoroutine::new(
            |_yielder: &OpenYielder<i32, i32>, param| {
                assert_eq!(0, param);
                1
            },
            0,
            2048,
        )
        .expect("create coroutine failed !");
        assert_eq!(1, coroutine.resume().as_return().unwrap());
    }

    // #[test]
    // fn test_yield_once() {
    //     // will panic
    //     let coroutine = OpenCoroutine::new(
    //         |yielder: &Yielder<i32, i32>, input| {
    //             assert_eq!(1, input);
    //             assert_eq!(3, yielder.suspend(2));
    //             6
    //         },
    //         1,
    //         2048,
    //     )
    //     .expect("create coroutine failed !");
    //     assert_eq!(2, coroutine.resume().as_yield().unwrap());
    // }

    #[test]
    fn test_yield() {
        let coroutine = OpenCoroutine::new(
            |yielder, input| {
                assert_eq!(1, input);
                assert_eq!(3, yielder.suspend(2));
                assert_eq!(5, yielder.suspend(4));
                6
            },
            1,
            2048,
        )
        .expect("create coroutine failed !");
        assert_eq!(2, coroutine.resume().as_yield().unwrap());
        assert_eq!(4, coroutine.resume_with(3).1.as_yield().unwrap());
        assert_eq!(6, coroutine.resume_with(5).1.as_return().unwrap());
    }

    #[test]
    fn test_current() {
        assert!(OpenCoroutine::<i32, i32, i32>::current().is_none());
        let coroutine = OpenCoroutine::new(
            |_yielder: &OpenYielder<i32, i32>, input| {
                assert_eq!(0, input);
                assert!(OpenCoroutine::<i32, i32, i32>::current().is_some());
                1
            },
            0,
            2048,
        )
        .expect("create coroutine failed !");
        assert_eq!(coroutine.resume().as_return().unwrap(), 1);
    }
}
