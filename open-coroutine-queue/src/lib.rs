//! Suppose a thread in a work-stealing scheduler is idle and looking for the next task to run. To
//! find an available task, it might do the following:
//!
//! 1. Try popping one task from the local worker queue.
//! 2. Try popping and stealing tasks from another local worker queue.
//! 3. Try popping and stealing a batch of tasks from the global injector queue.
//!
//! An implementation of this work-stealing strategy:
//!
//! # Examples
//!
//! ```
//! use open_coroutine_queue::WorkStealQueue;
//!
//! let queue = WorkStealQueue::new(2, 64);
//! queue.push(6);
//! queue.push(7);
//!
//! let local0 = queue.local_queue();
//! local0.push_back(2);
//! local0.push_back(3);
//! local0.push_back(4);
//! local0.push_back(5);
//!
//! let local1 = queue.local_queue();
//! local1.push_back(0);
//! local1.push_back(1);
//! for i in 0..8 {
//!     assert_eq!(local1.pop_front(), Some(i));
//! }
//! assert_eq!(local0.pop_front(), None);
//! assert_eq!(local1.pop_front(), None);
//! assert_eq!(queue.pop(), None);
//! ```
//!

pub use rand::*;
pub use work_steal::*;

pub mod rand;

pub mod work_steal;
