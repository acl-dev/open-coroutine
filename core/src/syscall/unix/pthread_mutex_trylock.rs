use libc::pthread_mutex_t;
use once_cell::sync::Lazy;
use std::ffi::c_int;

#[must_use]
pub extern "C" fn pthread_mutex_trylock(
    fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
    lock: *mut pthread_mutex_t,
) -> c_int {
    static CHAIN: Lazy<PthreadMutexTrylockSyscallFacade<RawPthreadMutexTrylockSyscall>> =
        Lazy::new(Default::default);
    CHAIN.pthread_mutex_trylock(fn_ptr, lock)
}

trait PthreadMutexTrylockSyscall {
    extern "C" fn pthread_mutex_trylock(
        &self,
        fn_ptr: Option<&extern "C" fn(*mut pthread_mutex_t) -> c_int>,
        lock: *mut pthread_mutex_t,
    ) -> c_int;
}

impl_facade!(PthreadMutexTrylockSyscallFacade, PthreadMutexTrylockSyscall,
    pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int
);

impl_raw!(RawPthreadMutexTrylockSyscall, PthreadMutexTrylockSyscall,
    pthread_mutex_trylock(lock: *mut pthread_mutex_t) -> c_int
);
