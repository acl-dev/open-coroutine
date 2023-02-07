use once_cell::sync::Lazy;
use std::os::raw::c_void;
use work_steal_queue::{LocalQueue, WorkStealQueue};

static QUEUE: Lazy<WorkStealQueue<&'static mut c_void>> = Lazy::new(WorkStealQueue::default);

static LOCAL_QUEUES: Lazy<Box<[LocalQueue<&'static mut c_void>]>> =
    Lazy::new(|| (0..num_cpus::get()).map(|_| QUEUE.local_queue()).collect());

pub fn get_queue() -> &'static LocalQueue<'static, &'static mut c_void> {
    LOCAL_QUEUES.get(QUEUE.next_index()).unwrap()
}
