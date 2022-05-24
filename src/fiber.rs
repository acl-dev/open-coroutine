use std::os::raw::{c_int, c_uint, c_void};
use crate::libfiber::{ACL_FIBER, acl_fiber_check_timer, acl_fiber_create, acl_fiber_delay, acl_fiber_hook_api, acl_fiber_id, acl_fiber_kill, acl_fiber_killed, acl_fiber_running, acl_fiber_schedule_stop, acl_fiber_schedule_with, acl_fiber_scheduled, acl_fiber_status, acl_fiber_yield, size_t};

pub struct Fiber {
    fiber: *mut ACL_FIBER,
}

impl Fiber {
    /// 创建纤程
    pub fn new(function: unsafe extern "C" fn(fiber: *mut ACL_FIBER, arg: *mut c_void),
               arg: *mut c_void, size: size_t) -> Self {
        Fiber {
            fiber: unsafe {
                acl_fiber_create(Some(function), arg, size)
            }
        }
    }

    ///主动让出CPU给其它纤程
    pub fn yields(&self) {
        unsafe {
            acl_fiber_yield();
        }
    }

    ///获取当前运行的纤程，如果没有正在运行的纤程将返回null
    pub fn current_running_fiber() -> *mut ACL_FIBER {
        unsafe {
            acl_fiber_running()
        }
    }

    ///获取指定纤程的id
    pub fn get_id(&self) -> c_uint {
        unsafe {
            acl_fiber_id(self.fiber)
        }
    }

    ///获取指定纤程的状态
    pub fn get_status(&self) -> c_int {
        unsafe {
            acl_fiber_status(self.fiber)
        }
    }

    ///纤程退出
    pub fn exit(&self) {
        unsafe {
            acl_fiber_kill(self.fiber)
        }
    }

    ///检查指定的纤程是否已经退出
    pub fn is_exited(&self) -> bool {
        unsafe {
            acl_fiber_killed(self.fiber) > 0
        }
    }

    ///让当前纤程休眠一段时间
    pub fn delay(&self, milliseconds: c_uint) -> c_uint {
        unsafe {
            acl_fiber_delay(milliseconds)
        }
    }
}

pub struct Scheduler {
    mode: c_int,
}

impl Scheduler {
    pub fn new(mode: u32) -> Self {
        Scheduler { mode: mode as c_int }
    }

    pub fn start(&self) {
        unsafe {
            acl_fiber_schedule_with(self.mode);
        }
    }

    pub fn is_scheduling(&self) -> bool {
        unsafe {
            acl_fiber_scheduled() > 0
        }
    }

    pub fn stop(&self) {
        unsafe {
            acl_fiber_schedule_stop();
        }
    }

    pub fn clean(&self, size: size_t) {
        unsafe {
            acl_fiber_check_timer(size);
        }
    }
}

pub struct SystemCallHooks {}

impl SystemCallHooks {
    ///是否hook系统函数，默认不hook
    pub fn enable(enable: bool) {
        unsafe {
            acl_fiber_hook_api(match enable {
                true => 1,
                false => 0,
            });
        }
    }
}