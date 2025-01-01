#![deny(
    // The following are allowed by default lints according to
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html
    absolute_paths_not_starting_with_crate,
    explicit_outlives_requirements,
    macro_use_extern_crate,
    redundant_lifetimes,
    anonymous_parameters,
    bare_trait_objects,
    // elided_lifetimes_in_paths, // allow anonymous lifetime
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    // single_use_lifetimes, // TODO: fix lifetime names only used once
    // trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    // unsafe_code,
    unstable_features,
    // unused_crate_dependencies,
    unused_lifetimes,
    unused_macro_rules,
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
//! see `https://github.com/acl-dev/open-coroutine`

/// Common traits and impl.
pub mod common;

/// Configuration for `EventLoops`.
#[allow(missing_docs)]
pub mod config;

#[doc = include_str!("../docs/en/coroutine.md")]
pub mod coroutine;

/// Make the coroutine automatically yield.
#[cfg(all(unix, feature = "preemptive"))]
mod monitor;

/// Scheduler impls.
pub mod scheduler;

/// Coroutine pool abstraction and impl.
pub mod co_pool;

/// net abstraction and impl.
#[allow(dead_code)]
#[cfg(feature = "net")]
pub mod net;

/// Syscall impl.
#[allow(
    missing_docs,
    clippy::similar_names,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::many_single_char_names,
    clippy::useless_conversion,
    clippy::unnecessary_cast,
    trivial_numeric_casts
)]
#[cfg(feature = "syscall")]
pub mod syscall;
