use crate::scheduler::Scheduler;
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
pub struct EventLoop<'a> {
    name: &'a str,
    scheduler: &'a mut Scheduler<'a>,
    waiting: AtomicBool,
}
