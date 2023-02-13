//! Readiness event types and utilities.

#[allow(clippy::module_inception)]
mod event;
mod events;

pub use self::event::Event;
pub use self::events::{Events, Iter};
