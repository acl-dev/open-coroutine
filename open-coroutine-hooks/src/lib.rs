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

use open_coroutine_core::config::Config;
use open_coroutine_core::event_loop::EventLoops;

#[no_mangle]
pub extern "C" fn init_config(config: Config) {
    //一方面保证hook的函数能够被重定向到(防止压根不调用coroutine_crate的情况)
    //另一方面初始化EventLoop配置
    _ = Config::get_instance()
        .set_event_loop_size(config.get_event_loop_size())
        .set_stack_size(config.get_stack_size())
        .set_min_size(config.get_min_size())
        .set_max_size(config.get_max_size())
        .set_keep_alive_time(config.get_keep_alive_time());
    open_coroutine_core::warn!("open-coroutine inited with {config:#?}");
}

#[no_mangle]
pub extern "C" fn shutdowns() {
    EventLoops::stop();
}

pub mod coroutine;

#[allow(dead_code, clippy::not_unsafe_ptr_arg_deref, clippy::similar_names)]
#[cfg(unix)]
pub mod unix;

#[allow(dead_code, clippy::not_unsafe_ptr_arg_deref, clippy::similar_names)]
#[cfg(windows)]
mod windows;
