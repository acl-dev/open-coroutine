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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NANOS_PER_SEC: u64 = 1_000_000_000;

// get the current wall clock in ns
#[allow(clippy::cast_possible_truncation)]
#[must_use]
#[inline]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("1970-01-01 00:00:00 UTC was {} seconds ago!")
        .as_nanos() as u64
}

#[must_use]
#[inline]
pub fn dur_to_ns(dur: Duration) -> u64 {
    // Note that a duration is a (u64, u32) (seconds, nanoseconds) pair
    dur.as_secs()
        .saturating_mul(NANOS_PER_SEC)
        .saturating_add(u64::from(dur.subsec_nanos()))
}

#[must_use]
pub fn get_timeout_time(dur: Duration) -> u64 {
    add_timeout_time(dur_to_ns(dur))
}

#[must_use]
pub fn add_timeout_time(time: u64) -> u64 {
    let now = now();
    match now.checked_add(time) {
        Some(time) => time,
        //处理溢出
        None => u64::MAX,
    }
}

mod typed;

pub use typed::*;

#[cfg(test)]
mod tests {
    use crate::now;

    #[test]
    fn test() {
        println!("{}", now());
    }
}
