use crate::coroutine::suspender::Suspender;
use crate::scheduler::Scheduler;
use corosensei::stack::DefaultStack;
use corosensei::ScopedCoroutine;
use std::cell::{Cell, RefCell};
use std::fmt::{Debug, Formatter};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod suspender;

#[allow(clippy::pedantic)]
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

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CoroutineState {
    ///协程被创建
    Created,
    ///等待运行
    Ready,
    ///运行中
    Running,
    ///被挂起，参数为开始执行的时间戳
    Suspend(u64),
    ///执行系统调用
    SystemCall,
    ///栈扩/缩容时
    CopyStack,
    ///执行用户函数完成
    Finished,
}

#[repr(C)]
pub struct Coroutine<'c, 's, Param, Yield, Return> {
    name: &'c str,
    sp: RefCell<ScopedCoroutine<'c, Param, Yield, Return, DefaultStack>>,
    status: Cell<CoroutineState>,
    //调用用户函数的参数
    param: RefCell<Param>,
    //调用用户函数的返回值
    result: RefCell<MaybeUninit<ManuallyDrop<Return>>>,
    scheduler: RefCell<Option<&'c Scheduler<'s>>>,
}

#[macro_export]
macro_rules! co {
    ($f:expr, $param:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            $param,
            page_size(),
        )
    };
    ($f:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(
            Box::from(uuid::Uuid::new_v4().to_string()),
            $f,
            (),
            page_size(),
        )
    };
    ($name:literal, $f:expr, $param:expr, $size:literal $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from($name), $f, $param, $size)
    };
    ($name:literal, $f:expr, $param:expr $(,)?) => {
        $crate::coroutine::Coroutine::new(Box::from($name), $f, $param, page_size())
    };
}

impl<'c, 's, Param, Yield, Return> Coroutine<'c, 's, Param, Yield, Return> {
    pub fn new<F>(name: Box<str>, f: F, param: Param, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&Suspender<Param, Yield>, Param) -> Return,
        F: 'c,
    {
        let stack = DefaultStack::new(size)?;
        let sp = ScopedCoroutine::with_stack(stack, |y, p| {
            let suspender = Suspender::new(y);
            Suspender::<Param, Yield>::init_current(&suspender);
            let r = f(&suspender, p);
            Suspender::<Param, Yield>::clean_current();
            r
        });
        Ok(Coroutine {
            name: Box::leak(name),
            sp: RefCell::new(sp),
            status: Cell::new(CoroutineState::Created),
            param: RefCell::new(param),
            result: RefCell::new(MaybeUninit::uninit()),
            scheduler: RefCell::new(None),
        })
    }
}

impl<'c, 's, Param, Yield, Return> Debug for Coroutine<'c, 's, Param, Yield, Return> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine")
            .field("name", &self.name)
            .field("status", &self.status)
            .field("scheduler", &self.scheduler)
            .finish()
    }
}
