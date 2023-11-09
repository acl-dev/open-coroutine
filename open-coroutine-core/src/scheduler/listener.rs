use crate::constants::{Syscall, SyscallState};
use crate::scheduler::SchedulableCoroutine;
use std::fmt::Debug;

#[allow(unused_variables)]
pub trait Listener: Debug {
    fn on_create(&self, co: &SchedulableCoroutine) {}
    fn on_suspend(&self, co: &SchedulableCoroutine) {}
    fn on_syscall(&self, co: &SchedulableCoroutine, syscall: Syscall, state: SyscallState) {}
    fn on_finish(&self, co: &SchedulableCoroutine) {}
}
