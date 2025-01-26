use libc::pthread_mutex_t;
use std::ffi::c_int;

trait PthreadMutexTrylockSyscall {
    extern "C" fn pthread_mutex_trylock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int;
}

impl_syscall!(PthreadMutexTrylockSyscallFacade, RawPthreadMutexTrylockSyscall,
    pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int
);

impl_facade!(PthreadMutexTrylockSyscallFacade, PthreadMutexTrylockSyscall,
    pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int
);

impl_raw!(RawPthreadMutexTrylockSyscall, PthreadMutexTrylockSyscall,
    pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int
);
