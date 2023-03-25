#[repr(C)]
#[derive(Debug, Eq, PartialEq)]
pub struct Scheduler<'s> {
    name: &'s str,
}
