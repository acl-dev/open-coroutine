mod afd;

pub mod event;
pub use event::{Event, Events};

mod handle;
use handle::Handle;

mod io_status_block;
mod iocp;

mod overlapped;
use overlapped::Overlapped;

mod selector;
pub use selector::{Selector, SelectorInner, SockState};

use std::io;
use std::os::windows::io::RawSocket;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::event_loop::interest::Interest;

struct InternalState {
    selector: Arc<SelectorInner>,
    token: usize,
    interests: Interest,
    sock_state: Pin<Arc<Mutex<SockState>>>,
}

impl Drop for InternalState {
    fn drop(&mut self) {
        let mut sock_state = self.sock_state.lock().unwrap();
        sock_state.mark_delete();
    }
}
