use std::fmt::Debug;
use std::time::Duration;

pub trait Blocker: Debug {
    fn block(&self, time: Duration);
}
