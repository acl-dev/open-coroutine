use crate::scheduler::SchedulableCoroutine;

#[derive(Debug)]
pub(crate) struct TaskNode {
    pthread: libc::pthread_t,
    coroutine: Option<*const SchedulableCoroutine>,
}

impl TaskNode {
    pub fn new(pthread: libc::pthread_t, coroutine: Option<*const SchedulableCoroutine>) -> Self {
        TaskNode { pthread, coroutine }
    }

    pub fn get_pthread(&self) -> libc::pthread_t {
        self.pthread
    }

    pub fn get_coroutine(&self) -> Option<*const SchedulableCoroutine> {
        self.coroutine
    }
}

impl Eq for TaskNode {}

impl PartialEq<Self> for TaskNode {
    fn eq(&self, other: &Self) -> bool {
        self.pthread.eq(&other.pthread)
    }
}

impl PartialOrd<Self> for TaskNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pthread.cmp(&other.pthread)
    }
}
