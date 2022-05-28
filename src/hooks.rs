use crate::libfiber::acl_fiber_hook_api;

pub struct Hooks {}

impl Hooks {
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