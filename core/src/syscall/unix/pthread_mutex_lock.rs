use crate::net::EventLoops;
use crate::scheduler::SchedulableCoroutine;
use libc::pthread_mutex_t;
use std::ffi::c_int;

trait PthreadMutexLockSyscall {
    extern "C" fn pthread_mutex_lock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int;
}

impl_syscall!(PthreadMutexLockSyscallFacade, NioPthreadMutexLockSyscall, RawPthreadMutexLockSyscall,
    pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int
);

impl_facade!(PthreadMutexLockSyscallFacade, PthreadMutexLockSyscall,
    pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int
);

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
struct NioPthreadMutexLockSyscall<I: PthreadMutexLockSyscall> {
    inner: I,
}

impl<I: PthreadMutexLockSyscall> PthreadMutexLockSyscall for NioPthreadMutexLockSyscall<I> {
    extern "C" fn pthread_mutex_lock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int {
        if SchedulableCoroutine::current().is_none() {
            return self.inner.pthread_mutex_lock(fn_ptr, lock);
        }
        loop {
            let r = unsafe { libc::pthread_mutex_trylock(lock) };
            if 0 == r
                || r != libc::EBUSY
                || EventLoops::wait_event(Some(crate::common::constants::SLICE)).is_err()
            {
                return r;
            }
        }
    }
}

impl_raw!(RawPthreadMutexLockSyscall, PthreadMutexLockSyscall,
    pthread_mutex_lock(lock: *mut pthread_mutex_t) -> c_int
);
