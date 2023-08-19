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
pub mod log;

pub mod coroutine;

pub mod scheduler;

pub mod pool;

#[macro_export]
macro_rules! unbreakable {
    ( $f: expr , $syscall: expr ) => {{
        $crate::info!("{} hooked", $syscall);
        if $crate::coroutine::suspender::Suspender::<(), ()>::current().is_some() {
            let co = $crate::scheduler::SchedulableCoroutine::current()
                .unwrap_or_else(|| panic!("current coroutine not found !"));
            let co_name = co.get_name();
            let state = co.set_state($crate::coroutine::CoroutineState::SystemCall($syscall));
            assert_eq!($crate::coroutine::CoroutineState::Running, state);
            let r = $f;
            if let Some(current) = $crate::scheduler::SchedulableCoroutine::current() {
                if co_name == current.get_name() {
                    let old = current.set_state(state);
                    match old {
                        $crate::coroutine::CoroutineState::SystemCall(_) => {}
                        _ => panic!("{} unexpected state {old}", current.get_name()),
                    };
                }
            }
            r
        } else {
            $f
        }
    }};
}

#[cfg(all(unix, feature = "preemptive-schedule"))]
mod monitor;

#[allow(
    dead_code,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    trivial_numeric_casts
)]
pub mod event_loop;

pub mod config;
