use crate::common::now;
use crate::net::EventLoops;
use libc::{pthread_cond_t, pthread_mutex_t, timespec};
use once_cell::sync::Lazy;
use std::ffi::c_int;
use std::time::Duration;

#[must_use]
pub extern "C" fn pthread_cond_timedwait(
    fn_ptr: Option<
        &extern "C" fn(*mut pthread_cond_t, *mut pthread_mutex_t, *const timespec) -> c_int,
    >,
    cond: *mut pthread_cond_t,
    lock: *mut pthread_mutex_t,
    abstime: *const timespec,
) -> c_int {
    static CHAIN: Lazy<
        PthreadCondTimedwaitSyscallFacade<
            NioPthreadCondTimedwaitSyscall<RawPthreadCondTimedwaitSyscall>,
        >,
    > = Lazy::new(Default::default);
    CHAIN.pthread_cond_timedwait(fn_ptr, cond, lock, abstime)
}

trait PthreadCondTimedwaitSyscall {
    extern "C" fn pthread_cond_timedwait(
        &self,
        fn_ptr: Option<
            &extern "C" fn(*mut pthread_cond_t, *mut pthread_mutex_t, *const timespec) -> c_int,
        >,
        cond: *mut pthread_cond_t,
        lock: *mut pthread_mutex_t,
        abstime: *const timespec,
    ) -> c_int;
}

impl_facade!(PthreadCondTimedwaitSyscallFacade, PthreadCondTimedwaitSyscall,
    pthread_cond_timedwait(
        cond: *mut pthread_cond_t,
        lock: *mut pthread_mutex_t,
        abstime: *const timespec
    ) -> c_int
);

#[repr(C)]
#[derive(Debug, Default)]
struct NioPthreadCondTimedwaitSyscall<I: PthreadCondTimedwaitSyscall> {
    inner: I,
}

impl<I: PthreadCondTimedwaitSyscall> PthreadCondTimedwaitSyscall
    for NioPthreadCondTimedwaitSyscall<I>
{
    extern "C" fn pthread_cond_timedwait(
        &self,
        fn_ptr: Option<
            &extern "C" fn(*mut pthread_cond_t, *mut pthread_mutex_t, *const timespec) -> c_int,
        >,
        cond: *mut pthread_cond_t,
        lock: *mut pthread_mutex_t,
        abstime: *const timespec,
    ) -> c_int {
        #[cfg(all(unix, feature = "preemptive"))]
        if crate::monitor::Monitor::current().is_some() {
            return self.inner.pthread_cond_timedwait(
                fn_ptr,
                cond,
                lock,
                abstime,
            );
        }
        let abstimeout = if abstime.is_null() {
            u64::MAX
        } else {
            let abstime = unsafe { *abstime };
            if abstime.tv_sec < 0 || abstime.tv_nsec < 0 || abstime.tv_nsec > 999_999_999 {
                return libc::EINVAL;
            }
            u64::try_from(Duration::new(
                    abstime.tv_sec.try_into().expect("overflow"),
                    abstime.tv_nsec.try_into().expect("overflow")
                ).as_nanos()
            ).unwrap_or(u64::MAX)
        };
        loop {
            let mut left_time = abstimeout.saturating_sub(now());
            if 0 == left_time {
                return libc::ETIMEDOUT;
            }
            let r = self.inner.pthread_cond_timedwait(
                fn_ptr,
                cond,
                lock,
                &timespec {
                    tv_sec: now().saturating_div(1_000_000_000).saturating_add(1).try_into().expect("overflow"),
                    tv_nsec: 0,
                },
            );
            if libc::ETIMEDOUT != r {
                return r;
            }
            left_time = abstimeout.saturating_sub(now());
            if 0 == left_time {
                return libc::ETIMEDOUT;
            }
            let wait_time = if left_time > 10_000_000 {
                10_000_000
            } else {
                left_time
            };
            if EventLoops::wait_event(Some(Duration::new(
                wait_time / 1_000_000_000,
                (wait_time % 1_000_000_000) as _,
            )))
            .is_err()
            {
                return r;
            }
        }
    }
}

impl_raw!(RawPthreadCondTimedwaitSyscall, PthreadCondTimedwaitSyscall,
    pthread_cond_timedwait(
        cond: *mut pthread_cond_t,
        lock: *mut pthread_mutex_t,
        abstime: *const timespec
    ) -> c_int
);
