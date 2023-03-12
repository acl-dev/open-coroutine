use std::sync::atomic::AtomicBool;

#[repr(C)]
#[derive(Debug)]
pub struct Scheduler<'s> {
    name: &'s str,
    scheduling: AtomicBool,
}
