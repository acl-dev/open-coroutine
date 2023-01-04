mod afd;

pub mod event;
pub use event::{Event, Events};
use std::os::windows::io::RawSocket;

mod handle;

use handle::Handle;

mod io_status_block;
mod iocp;

mod overlapped;

use overlapped::Overlapped;

mod selector;

pub use selector::{SelectorInner, SockState};

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::event_loop::interest::Interest;
pub use crate::event_loop::selector::windows::selector::Selector as InnerSelector;

pub struct InternalState {
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

pub type Selector = IoSourceState;

pub struct IoSourceState {
    // This is `None` if the socket has not yet been registered.
    //
    // We box the internal state to not increase the size on the stack as the
    // type might move around a lot.
    inner: Option<Box<InternalState>>,
    selector: InnerSelector,
}

impl IoSourceState {
    pub fn new() -> std::io::Result<IoSourceState> {
        Ok(IoSourceState {
            inner: None,
            selector: InnerSelector::new()?,
        })
    }

    pub fn select(
        &mut self,
        events: &mut crate::event_loop::event::Events,
        timeout: Option<Duration>,
    ) -> std::io::Result<()> {
        self.selector.select(events, timeout)
    }

    pub fn register(
        &mut self,
        socket: libc::c_int,
        token: usize,
        interests: Interest,
    ) -> std::io::Result<()> {
        if self.inner.is_some() {
            Err(std::io::ErrorKind::AlreadyExists.into())
        } else {
            self.selector
                .register(socket as RawSocket, token, interests)
                .map(|state| {
                    self.inner = Some(Box::new(state));
                })
        }
    }

    pub fn reregister(
        &mut self,
        _socket: libc::c_int,
        token: usize,
        interests: Interest,
    ) -> std::io::Result<()> {
        match self.inner.as_mut() {
            Some(state) => self
                .selector
                .reregister(state.sock_state.clone(), token, interests)
                .map(|()| {
                    state.token = token;
                    state.interests = interests;
                }),
            None => Err(std::io::ErrorKind::NotFound.into()),
        }
    }

    pub fn deregister(&mut self, _socket: libc::c_int) -> std::io::Result<()> {
        match self.inner.as_mut() {
            Some(state) => {
                {
                    let mut sock_state = state.sock_state.lock().unwrap();
                    sock_state.mark_delete();
                }
                self.inner = None;
                Ok(())
            }
            None => Err(std::io::ErrorKind::NotFound.into()),
        }
    }
}
