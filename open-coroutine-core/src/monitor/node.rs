use nix::sys::pthread::{pthread_self, Pthread};

#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(super) struct NotifyNode {
    timestamp: u64,
    pthread: Pthread,
}

impl NotifyNode {
    pub fn new(timestamp: u64) -> Self {
        NotifyNode {
            timestamp,
            pthread: pthread_self(),
        }
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn pthread(&self) -> Pthread {
        self.pthread
    }
}
