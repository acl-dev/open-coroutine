use crate::common::Named;
use crate::constants::{CoroutineState, DEFAULT_STACK_SIZE};
use crate::coroutine::suspender::Suspender;
use crate::coroutine::{Coroutine, CoroutineImpl, SimpleCoroutine, StateCoroutine};
use crate::scheduler::listener::Listener;
use once_cell::sync::Lazy;
use open_coroutine_queue::LocalQueue;
use open_coroutine_timer::TimerList;
use std::collections::{HashMap, VecDeque};
use std::panic::UnwindSafe;
use std::time::Duration;
use uuid::Uuid;

pub mod listener;

#[cfg(test)]
mod tests;

/// 用户协程
pub type SchedulableCoroutine = CoroutineImpl<'static, (), (), Option<usize>>;

static mut SUSPEND_TABLE: Lazy<TimerList<SchedulableCoroutine>> = Lazy::new(TimerList::default);

static mut SYSTEM_CALL_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

static mut RESULT_TABLE: Lazy<HashMap<&str, SchedulableCoroutine>> = Lazy::new(HashMap::new);

#[repr(C)]
#[derive(Debug)]
pub struct SchedulerImpl<'s> {
    name: &'s str,
    ready: LocalQueue<'s, SchedulableCoroutine>,
    listeners: VecDeque<Box<dyn Listener>>,
}

impl Drop for SchedulerImpl<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.ready.is_empty(),
                "there are still coroutines to be carried out !"
            );
        }
    }
}

impl SchedulerImpl<'_> {
    #[must_use]
    pub fn new() -> Self {
        Self::with_name(Box::from(Uuid::new_v4().to_string()))
    }

    #[must_use]
    pub fn with_name(name: Box<str>) -> Self {
        SchedulerImpl {
            name: Box::leak(name),
            ready: LocalQueue::default(),
            listeners: VecDeque::default(),
        }
    }

    pub fn submit_co(
        &self,
        f: impl FnOnce(&dyn Suspender<Resume = (), Yield = ()>, ()) -> Option<usize>
            + UnwindSafe
            + 'static,
        stack_size: Option<usize>,
    ) -> std::io::Result<&'static str> {
        let coroutine = SchedulableCoroutine::new(
            format!("{}|{}", self.name, Uuid::new_v4()),
            f,
            stack_size.unwrap_or(DEFAULT_STACK_SIZE),
        )?;
        assert_eq!(
            CoroutineState::Created,
            coroutine.change_state(CoroutineState::Ready)
        );
        let co_name = Box::leak(Box::from(coroutine.get_name()));
        self.on_create(&coroutine);
        self.ready.push_back(coroutine);
        Ok(co_name)
    }

    fn check_ready(&self) {
        unsafe {
            for _ in 0..SUSPEND_TABLE.len() {
                if let Some((exec_time, _)) = SUSPEND_TABLE.front() {
                    if open_coroutine_timer::now() < *exec_time {
                        break;
                    }
                    //移动至"就绪"队列
                    if let Some((_, mut entry)) = SUSPEND_TABLE.pop_front() {
                        for _ in 0..entry.len() {
                            if let Some(coroutine) = entry.pop_front() {
                                let old = coroutine.change_state(CoroutineState::Ready);
                                match old {
                                    CoroutineState::Suspend((), _) => {}
                                    _ => panic!("{} unexpected state {old}", coroutine.get_name()),
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
        self.try_timeout_schedule(open_coroutine_timer::get_timeout_time(time))
    }

    pub fn try_timeout_schedule(&self, timeout_time: u64) -> u64 {
        self.on_schedule(timeout_time);
        loop {
            let left_time = timeout_time.saturating_sub(open_coroutine_timer::now());
            if left_time == 0 {
                return 0;
            }
            self.check_ready();
            match self.ready.pop_front() {
                Some(mut coroutine) => {
                    cfg_if::cfg_if! {
                        if #[cfg(all(unix, feature = "preemptive-schedule"))] {
                            let start = open_coroutine_timer::get_timeout_time(Duration::from_millis(10))
                                .min(timeout_time);
                            crate::monitor::Monitor::add_task(start, Some(&coroutine));
                        }
                    }
                    self.on_resume(timeout_time, &coroutine);
                    match coroutine.resume().unwrap() {
                        CoroutineState::Suspend((), timestamp) => {
                            self.on_suspend(timeout_time, &coroutine);
                            if timestamp > 0 {
                                //挂起协程到时间轮
                                unsafe { SUSPEND_TABLE.insert(timestamp, coroutine) };
                            } else {
                                //放入就绪队列尾部
                                self.ready.push_back(coroutine);
                            }
                        }
                        CoroutineState::SystemCall((), syscall, state) => {
                            self.on_syscall(timeout_time, &coroutine, syscall, state);
                            //挂起协程到系统调用表
                            let co_name = Box::leak(Box::from(coroutine.get_name()));
                            //如果已包含，说明当前系统调用还有上层父系统调用，因此直接忽略插入结果
                            unsafe { _ = SYSTEM_CALL_TABLE.insert(co_name, coroutine) };
                        }
                        CoroutineState::Complete(result) => {
                            self.on_complete(timeout_time, &coroutine, result);
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

    pub fn add_listener(&mut self, listener: impl Listener + 'static) {
        self.listeners.push_back(Box::new(listener));
    }

    //只有框架级crate才需要使用此方法
    pub fn resume_syscall(&self, co_name: &'static str) {
        unsafe {
            if let Some(coroutine) = SYSTEM_CALL_TABLE.remove(&co_name) {
                self.ready.push_back(coroutine);
            }
        }
    }

    pub fn get_result(co_name: &'static str) -> Option<SchedulableCoroutine> {
        unsafe { RESULT_TABLE.remove(&co_name) }
    }
}

impl Default for SchedulerImpl<'_> {
    fn default() -> Self {
        Self::new()
    }
}
