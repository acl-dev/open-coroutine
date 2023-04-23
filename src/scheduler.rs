use crate::coroutine::suspender::Suspender;
use crate::coroutine::{Coroutine, CoroutineState};
use corosensei::stack::DefaultStack;
use corosensei::ScopedCoroutine;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Duration;
use timer_utils::TimerList;
use uuid::Uuid;
use work_steal_queue::{LocalQueue, WorkStealQueue};

/// 源协程
#[allow(dead_code)]
type RootCoroutine<'a> = ScopedCoroutine<'a, (), (), (), DefaultStack>;

/// 用户协程
pub type SchedulableCoroutine = Coroutine<'static, (), (), &'static mut c_void>;

static QUEUE: Lazy<WorkStealQueue<SchedulableCoroutine>> = Lazy::new(WorkStealQueue::default);

static mut SUSPEND_TABLE: Lazy<TimerList<SchedulableCoroutine>> = Lazy::new(TimerList::new);

static mut SYSTEM_CALL_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

#[allow(dead_code)]
static mut COPY_STACK_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

static mut RESULT_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler {
    name: &'static str,
    ready: LocalQueue<'static, SchedulableCoroutine>,
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        assert!(
            self.ready.is_empty(),
            "there are still tasks to be carried out !"
        );
    }
}

impl Scheduler {
    #[must_use]
    pub fn new() -> Self {
        Self::with_name(Box::from(Uuid::new_v4().to_string()))
    }

    pub fn with_name(name: Box<str>) -> Self {
        Scheduler {
            name: Box::leak(name),
            ready: QUEUE.local_queue(),
        }
    }

    pub fn submit(
        &self,
        f: impl FnOnce(&Suspender<'_, (), ()>, ()) -> &'static mut c_void + 'static,
    ) -> std::io::Result<&'static str> {
        let coroutine = SchedulableCoroutine::new(
            Box::from(format!("{}|{}", self.name, Uuid::new_v4())),
            f,
            crate::coroutine::default_stack_size(),
        )?;
        assert_eq!(
            CoroutineState::Created,
            coroutine.set_state(CoroutineState::Ready)
        );
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.ready.push_back(coroutine);
        Ok(co_name)
    }

    fn check_ready(&self) {
        unsafe {
            for _ in 0..SUSPEND_TABLE.len() {
                if let Some(entry) = SUSPEND_TABLE.front() {
                    let exec_time = entry.get_time();
                    if timer_utils::now() < exec_time {
                        break;
                    }
                    //移动至"就绪"队列
                    if let Some(mut entry) = SUSPEND_TABLE.pop_front() {
                        for _ in 0..entry.len() {
                            if let Some(coroutine) = entry.pop_front() {
                                match coroutine.set_state(CoroutineState::Ready) {
                                    CoroutineState::Suspend(_) => {}
                                    _ => panic!("unexpected state"),
                                };
                                //把到时间的协程加入就绪队列
                                self.ready.push_back(coroutine);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn try_schedule(&self) {
        _ = self.try_timeout_schedule(Duration::MAX.as_secs());
    }

    pub fn try_timed_schedule(&self, time: Duration) -> u64 {
        self.try_timeout_schedule(timer_utils::get_timeout_time(time))
    }

    pub fn try_timeout_schedule(&self, timeout_time: u64) -> u64 {
        loop {
            let left_time = timeout_time.saturating_sub(timer_utils::now());
            if left_time == 0 {
                return 0;
            }
            self.check_ready();
            match self.ready.pop_front() {
                Some(coroutine) => {
                    _ = coroutine.set_scheduler(self);
                    cfg_if::cfg_if! {
                        if #[cfg(all(unix, feature = "preemptive-schedule"))] {
                            let start = timer_utils::get_timeout_time(Duration::from_millis(10));
                            crate::monitor::Monitor::add_task(start, Some(&coroutine));
                        }
                    }
                    match coroutine.resume() {
                        CoroutineState::Suspend(timestamp) => {
                            if timestamp > 0 {
                                //挂起协程到时间轮
                                unsafe { SUSPEND_TABLE.insert(timestamp, coroutine) };
                            } else {
                                //放入就绪队列尾部
                                self.ready.push_back(coroutine);
                            }
                        }
                        CoroutineState::SystemCall(_syscall_name) => {
                            //挂起协程到系统调用表
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            unsafe {
                                assert!(SYSTEM_CALL_TABLE.insert(co_name, coroutine).is_none());
                            }
                        }
                        CoroutineState::CopyStack => {
                            todo!()
                        }
                        CoroutineState::Finished => {
                            let name = Box::leak(Box::from(coroutine.get_name()));
                            _ = unsafe { RESULT_TABLE.insert(name, coroutine) };
                        }
                        _ => unreachable!("should never execute to here"),
                    };
                    cfg_if::cfg_if! {
                        if #[cfg(all(unix, feature = "preemptive-schedule"))] {
                            //还没执行到10ms就主动yield或者执行完毕了，此时需要清理任务
                            //否则下一个协程执行不到10ms就会被抢占调度
                            crate::monitor::Monitor::clean_task(start);
                        }
                    }
                }
                None => return left_time,
            }
        }
    }

    //只有框架级crate才需要使用此方法
    pub fn resume_syscall(&self, co_name: usize) {
        unsafe {
            let co_name = Box::leak(Box::new(std::ptr::read_unaligned(
                (co_name as *const c_void).cast::<String>(),
            )))
            .as_str();
            if let Some(coroutine) = SYSTEM_CALL_TABLE.remove(&co_name) {
                match coroutine.set_state(CoroutineState::Ready) {
                    CoroutineState::SystemCall(_) => {}
                    _ => panic!("unexpected state"),
                };
                self.ready.push_back(coroutine);
            }
        }
    }

    pub fn get_result(co_name: &'static str) -> Option<SchedulableCoroutine> {
        unsafe { RESULT_TABLE.remove(&co_name) }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(result: usize) -> &'static mut c_void {
        unsafe { std::mem::transmute(result) }
    }

    #[test]
    fn test_simple() {
        let scheduler = Scheduler::new();
        _ = scheduler.submit(|_, _| {
            println!("1");
            result(1)
        });
        _ = scheduler.submit(|_, _| {
            println!("2");
            result(2)
        });
        scheduler.try_schedule();
    }

    #[test]
    fn test_backtrace() {
        let scheduler = Scheduler::new();
        _ = scheduler.submit(|_, _| result(1));
        _ = scheduler.submit(|_, _| {
            println!("{:?}", backtrace::Backtrace::new());
            result(2)
        });
        scheduler.try_schedule();
    }

    #[test]
    fn with_suspend() {
        let scheduler = Scheduler::new();
        _ = scheduler.submit(|suspender, _| {
            println!("[coroutine1] suspend");
            suspender.suspend();
            println!("[coroutine1] back");
            result(1)
        });
        _ = scheduler.submit(|suspender, _| {
            println!("[coroutine2] suspend");
            suspender.suspend();
            println!("[coroutine2] back");
            result(2)
        });
        scheduler.try_schedule();
    }

    #[test]
    fn with_delay() {
        let scheduler = Scheduler::new();
        _ = scheduler.submit(|suspender, _| {
            println!("[coroutine] delay");
            suspender.delay(Duration::from_millis(100));
            println!("[coroutine] back");
            result(1)
        });
        scheduler.try_schedule();
        std::thread::sleep(Duration::from_millis(100));
        scheduler.try_schedule();
    }

    #[cfg(all(
        target_os = "linux",
        target_os = "l4re",
        target_os = "android",
        target_os = "emscripten",
        feature = "preemptive-schedule"
    ))]
    #[test]
    fn preemptive_schedule() -> std::io::Result<()> {
        use std::sync::{Arc, Condvar, Mutex};
        static mut TEST_FLAG1: bool = true;
        static mut TEST_FLAG2: bool = true;
        let pair = Arc::new((Mutex::new(true), Condvar::new()));
        let pair2 = Arc::clone(&pair);
        let handler = std::thread::spawn(move || {
            let scheduler = Box::leak(Box::new(Scheduler::new()));
            _ = scheduler.submit(|_, _| {
                unsafe {
                    while TEST_FLAG1 {
                        _ = libc::usleep(10_000);
                    }
                }
                result(1)
            });
            _ = scheduler.submit(|_, _| {
                unsafe {
                    while TEST_FLAG2 {
                        _ = libc::usleep(10_000);
                    }
                }
                unsafe { TEST_FLAG1 = false };
                result(2)
            });
            _ = scheduler.submit(|_, _| {
                unsafe { TEST_FLAG2 = false };
                result(3)
            });
            scheduler.try_schedule();

            let (lock, cvar) = &*pair2;
            let mut pending = lock.lock().unwrap();
            *pending = false;
            // notify the condvar that the value has changed.
            cvar.notify_one();
        });

        // wait for the thread to start up
        let (lock, cvar) = &*pair;
        let result = cvar
            .wait_timeout_while(
                lock.lock().unwrap(),
                Duration::from_millis(3000),
                |&mut pending| pending,
            )
            .unwrap();
        if result.1.timed_out() {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "preemptive schedule failed",
            ))
        } else {
            unsafe {
                handler.join().unwrap();
                assert!(!TEST_FLAG1);
            }
            Ok(())
        }
    }
}
