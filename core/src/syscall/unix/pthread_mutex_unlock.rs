use libc::pthread_mutex_t;
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn pthread_mutex_unlock(
    fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
    lock: *mut pthread_mutex_t,
) -> c_int {
    static CHAIN: Lazy<PthreadMutexUnlockSyscallFacade<RawPthreadMutexUnlockSyscall>> =
        Lazy::new(Default::default);
    CHAIN.pthread_mutex_unlock(fn_ptr, lock)
}

trait PthreadMutexUnlockSyscall {
    extern "C" fn pthread_mutex_unlock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int;
}

impl_facade!(PthreadMutexUnlockSyscallFacade, PthreadMutexUnlockSyscall,
    pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int
);

impl_raw!(RawPthreadMutexUnlockSyscall, PthreadMutexUnlockSyscall,
    pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int
);
