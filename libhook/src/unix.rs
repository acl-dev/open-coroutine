use base_coroutine::scheduler::Scheduler;
use std::time::Duration;

//sleep相关
#[no_mangle]
pub extern "C" fn sleep(secs: libc::c_uint) -> libc::c_uint {
    let rqtp = libc::timespec {
        tv_sec: secs as i64,
        tv_nsec: 0,
    };
    let mut rmtp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    nanosleep(&rqtp, &mut rmtp);
    rmtp.tv_sec as u32
}

#[no_mangle]
pub extern "C" fn usleep(secs: libc::c_uint) -> libc::c_int {
    let secs = secs as i64;
    let sec = secs / 1_000_000;
    let nsec = (secs % 1_000_000) * 1000;
    let rqtp = libc::timespec {
        tv_sec: sec,
        tv_nsec: nsec,
    };
    let mut rmtp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    nanosleep(&rqtp, &mut rmtp)
}

#[no_mangle]
static mut NANOSLEEP: Option<
    extern "C" fn(*const libc::timespec, *mut libc::timespec) -> libc::c_int,
> = None;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn nanosleep(rqtp: *const libc::timespec, rmtp: *mut libc::timespec) -> libc::c_int {
    let nanos_time = unsafe {
        let tv_sec = (*rqtp).tv_sec;
        let tv_nsec = (*rqtp).tv_nsec;
        if tv_sec < 0 || tv_nsec < 0 {
            return -1;
        }
        match (tv_sec as u64).checked_mul(1_000_000_000) {
            Some(v) => v.checked_add(tv_nsec as u64).unwrap_or(u64::MAX),
            None => u64::MAX,
        }
    };
    let nanos_time = Duration::from_nanos(nanos_time);
    let timeout_time = timer_utils::get_timeout_time(nanos_time);
    //fixme 这里会导致CPU空循环，先跑通demo
    //Scheduler::current().try_timed_schedule(nanos_time);
    Scheduler::current().timed_schedule(nanos_time);
    // 可能schedule完还剩一些时间，此时本地队列没有任务可做
    // 后续考虑work-steal，需要在Scheduler增加timed_schedule实现
    let schedule_finished_time = timer_utils::now();
    //处理溢出的情况
    let left_time = match timeout_time.checked_sub(schedule_finished_time) {
        Some(v) => v,
        None => {
            if !rmtp.is_null() {
                unsafe {
                    (*rmtp).tv_sec = 0;
                    (*rmtp).tv_nsec = 0;
                }
            }
            return 0;
        }
    } as i64;
    let sec = left_time / 1_000_000_000;
    let nsec = left_time % 1_000_000_000;
    let rqtp = libc::timespec {
        tv_sec: sec,
        tv_nsec: nsec,
    };
    //获取原始系统函数nanosleep
    let original = unsafe {
        match NANOSLEEP {
            Some(original) => original,
            None => {
                let original =
                    std::mem::transmute::<
                        _,
                        extern "C" fn(*const libc::timespec, *mut libc::timespec) -> libc::c_int,
                    >(libc::dlsym(libc::RTLD_NEXT, "nanosleep".as_ptr() as _));
                NANOSLEEP = Some(original);
                original
            }
        }
    };
    //fixme 这里在linux环境下调用真实系统函数会报错
    //相当于libc::nanosleep(&rqtp, rmtp)
    original(&rqtp, rmtp)
}
