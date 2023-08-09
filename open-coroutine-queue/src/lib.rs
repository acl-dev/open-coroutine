#![deny(
    // The following are allowed by default lints according to
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html
    anonymous_parameters,
    bare_trait_objects,
    // box_pointers, // use box pointer to allocate on heap
    // elided_lifetimes_in_paths, // allow anonymous lifetime
    missing_copy_implementations,
    missing_debug_implementations,
    // missing_docs, // TODO: add documents
    // single_use_lifetimes, // TODO: fix lifetime names only used once
    // trivial_casts,
    trivial_numeric_casts,
    // unreachable_pub, allow clippy::redundant_pub_crate lint instead
    // unsafe_code,
    unstable_features,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    variant_size_differences,

    warnings, // treat all wanings as errors

    clippy::all,
    // clippy::restriction,
    clippy::pedantic,
    // clippy::nursery, // It's still under development
    clippy::cargo,
)]
#![allow(
    // Some explicitly allowed Clippy lints, must have clear reason to allow
    clippy::blanket_clippy_restriction_lints, // allow clippy::restriction
    clippy::implicit_return, // actually omitting the return keyword is idiomatic Rust code
    clippy::module_name_repetitions, // repeation of module name in a struct name is not big deal
    clippy::multiple_crate_versions, // multi-version dependency crates is not able to fix
    clippy::missing_errors_doc, // TODO: add error docs
    clippy::missing_panics_doc, // TODO: add panic docs
    clippy::panic_in_result_fn,
    clippy::shadow_same, // Not too much bad
    clippy::shadow_reuse, // Not too much bad
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::indexing_slicing,
    clippy::separated_literal_suffix, // conflicts with clippy::unseparated_literal_suffix
    clippy::single_char_lifetime_names, // TODO: change lifetime names
)]

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
