#[repr(C)]
#[derive(Debug)]
pub struct Scheduler<'s> {
    name: &'s str,
}
