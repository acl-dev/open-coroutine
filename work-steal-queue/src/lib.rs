//! Concurrent work-stealing deques.
//!
//! These data structures are most commonly used in work-stealing schedulers. The typical setup
//! involves a number of threads, each having its own FIFO or LIFO queue (*worker*). There is also
//! one global FIFO queue (*injector*) and a list of references to *worker* queues that are able to
//! steal tasks (*stealers*).
//!
//! We spawn a new task onto the scheduler by pushing it into the *injector* queue. Each worker
//! thread waits in a loop until it finds the next task to run and then runs it. To find a task, it
//! first looks into its local *worker* queue, and then into the *injector* and *stealers*.
//!
//! # Queues
//!
//! [`Injector`] is a FIFO queue, where tasks are pushed and stolen from opposite ends. It is
//! shared among threads and is usually the entry point for new tasks.
//!
//! [`Worker`] has two constructors:
//!
//! * [`new_fifo()`] - Creates a FIFO queue, in which tasks are pushed and popped from opposite
//!   ends.
//! * [`new_lifo()`] - Creates a LIFO queue, in which tasks are pushed and popped from the same
//!   end.
//!
//! Each [`Worker`] is owned by a single thread and supports only push and pop operations.
//!
//! Method [`stealer()`] creates a [`Stealer`] that may be shared among threads and can only steal
//! tasks from its [`Worker`]. Tasks are stolen from the end opposite to where they get pushed.
//!
//! # Stealing
//!
//! Steal operations come in three flavors:
//!
//! 1. [`steal()`] - Steals one task.
//! 2. [`steal_batch()`] - Steals a batch of tasks and moves them into another worker.
//! 3. [`steal_batch_and_pop()`] - Steals a batch of tasks, moves them into another queue, and pops
//!    one task from that worker.
//!
//! In contrast to push and pop operations, stealing can spuriously fail with [`Steal::Retry`], in
//! which case the steal operation needs to be retried.
//!
//! # Examples
//!
//! Suppose a thread in a work-stealing scheduler is idle and looking for the next task to run. To
//! find an available task, it might do the following:
//!
//! 1. Try popping one task from the local worker queue.
//! 2. Try stealing a batch of tasks from the global injector queue.
//! 3. Try stealing one task from another thread using the stealer list.
//!
//! An implementation of this work-stealing strategy:
//!
//! # Examples
//!
//! ```
//! use work_steal_queue::WorkStealQueue;
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
//! ```
//!
//! [`new_fifo()`]: Worker::new_fifo
//! [`new_lifo()`]: Worker::new_lifo
//! [`stealer()`]: Worker::stealer
//! [`steal()`]: Stealer::steal
//! [`steal_batch()`]: Stealer::steal_batch
//! [`steal_batch_and_pop()`]: Stealer::steal_batch_and_pop

#![cfg_attr(not(feature = "std"), no_std)]

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "std")] {
        use crossbeam_epoch as epoch;
        use crossbeam_utils as utils;

        mod deque;
        pub use crate::deque::{Injector, Steal, Stealer, Worker};

        #[allow(dead_code)]
        pub mod rand;
        mod work_steal;
        pub use crate::work_steal::{WorkStealQueue, LocalQueue};
    }
}
