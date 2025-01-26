use libc::pthread_mutex_t;
use std::ffi::c_int;

trait PthreadMutexUnlockSyscall {
    extern "C" fn pthread_mutex_unlock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int;
}

impl_syscall!(PthreadMutexUnlockSyscallFacade, RawPthreadMutexUnlockSyscall,
    pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int
);

impl_facade!(PthreadMutexUnlockSyscallFacade, PthreadMutexUnlockSyscall,
    pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int
);

impl_raw!(RawPthreadMutexUnlockSyscall, PthreadMutexUnlockSyscall,
    pthread_mutex_unlock(lock: *mut pthread_mutex_t) -> c_int
);
