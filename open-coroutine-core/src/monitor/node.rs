use crate::scheduler::SchedulableCoroutine;

#[derive(Debug)]
pub(crate) struct TaskNode {
    #[cfg(windows)]
    pthread: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    pthread: libc::pthread_t,
    coroutine: Option<*const SchedulableCoroutine>,
}

#[allow(dead_code)]
impl TaskNode {
    #[cfg(unix)]
    pub fn new(pthread: libc::pthread_t, coroutine: Option<*const SchedulableCoroutine>) -> Self {
        TaskNode { pthread, coroutine }
    }

    #[cfg(windows)]
    pub fn new(
        pthread: windows_sys::Win32::Foundation::HANDLE,
        coroutine: Option<*const SchedulableCoroutine>,
    ) -> Self {
        TaskNode { pthread, coroutine }
    }

    #[cfg(unix)]
    pub fn get_pthread(&self) -> libc::pthread_t {
        self.pthread
    }

    #[cfg(windows)]
    pub fn get_pthread(&self) -> windows_sys::Win32::Foundation::HANDLE {
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
        self.pthread.partial_cmp(&other.pthread)
    }
}

impl Ord for TaskNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pthread.cmp(&other.pthread)
    }
}
