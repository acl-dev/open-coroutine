use crate::scheduler::Scheduler;
use std::sync::atomic::AtomicBool;

pub struct EventLoop<'a> {
    name: &'a str,
    scheduler: &'a mut Scheduler<'a>,
    waiting: AtomicBool,
}
