#![deny(
    // The following are allowed by default lints according to
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html
    anonymous_parameters,
    bare_trait_objects,
    // elided_lifetimes_in_paths, // allow anonymous lifetime
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs, // TODO: add documents
    single_use_lifetimes, // TODO: fix lifetime names only used once
    trivial_casts, // TODO: remove trivial casts in code
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
    unreachable_pub,
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
    clippy::test_attr_in_doctest,
    linker_messages,
)]
#![doc = include_str!("../docs/en/hook.md")]

use once_cell::sync::OnceCell;
use open_coroutine_core::co_pool::task::UserTaskFunc;
use open_coroutine_core::config::Config;
use open_coroutine_core::net::join::JoinHandle;
use open_coroutine_core::net::{EventLoops, UserFunc};
use open_coroutine_core::scheduler::SchedulableCoroutine;
use std::ffi::{c_int, c_longlong, c_uint};
use std::time::Duration;

static HOOK: OnceCell<bool> = OnceCell::new();

pub(crate) fn hook() -> bool {
    HOOK.get().map_or_else(|| false, |v| *v)
}

#[allow(
    dead_code,
    missing_docs,
    clippy::similar_names,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::many_single_char_names,
    clippy::unnecessary_cast
)]
pub mod syscall;

/// Start the framework.
#[no_mangle]
pub extern "C" fn open_coroutine_init(config: Config) -> c_int {
    EventLoops::init(&config);
    _ = HOOK.get_or_init(|| config.hook());
    0
}

/// Stop the framework.
#[no_mangle]
pub extern "C" fn open_coroutine_stop(secs: c_uint) -> c_int {
    if EventLoops::stop(Duration::from_secs(u64::from(secs))).is_ok() {
        return 0;
    }
    -1
}

///创建任务
#[no_mangle]
pub extern "C" fn task_crate(f: UserTaskFunc, param: usize, priority: c_longlong) -> JoinHandle {
    EventLoops::submit_task(
        None,
        move |p| Some(f(p.unwrap_or(0))),
        Some(param),
        Some(priority),
    )
}

///等待任务完成
#[no_mangle]
pub extern "C" fn task_join(handle: &JoinHandle) -> c_longlong {
    match handle.join() {
        Ok(ptr) => match ptr {
            Ok(ptr) => match ptr {
                Some(ptr) => c_longlong::try_from(ptr).expect("overflow"),
                None => 0,
            },
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}

///等待任务完成
#[no_mangle]
pub extern "C" fn task_timeout_join(handle: &JoinHandle, ns_time: u64) -> c_longlong {
    match handle.timeout_join(Duration::from_nanos(ns_time)) {
        Ok(ptr) => match ptr {
            Ok(ptr) => match ptr {
                Some(ptr) => c_longlong::try_from(ptr).expect("overflow"),
                None => 0,
            },
            Err(_) => -1,
        },
        Err(_) => -1,
    }
}

///如果当前协程栈不够，切换到新栈上执行
#[no_mangle]
pub extern "C" fn maybe_grow_stack(
    red_zone: usize,
    stack_size: usize,
    f: UserFunc,
    param: usize,
) -> c_longlong {
    let red_zone = if red_zone > 0 {
        red_zone
    } else {
        open_coroutine_core::common::default_red_zone()
    };
    let stack_size = if stack_size > 0 {
        stack_size
    } else {
        open_coroutine_core::common::constants::DEFAULT_STACK_SIZE
    };
    if let Ok(r) = SchedulableCoroutine::maybe_grow_with(red_zone, stack_size, || f(param)) {
        return c_longlong::try_from(r).expect("overflow");
    }
    -1
}
