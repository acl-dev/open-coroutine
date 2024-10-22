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
)]
//! see `https://github.com/acl-dev/open-coroutine`

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitBool, LitInt};

/// use this macro like `#[open_coroutine::main(event_loop_size = 2, max_size = 2, keep_alive_time = 0)]`.
#[proc_macro_attribute]
pub fn main(args: TokenStream, func: TokenStream) -> TokenStream {
    let mut event_loop_size = usize::MAX;
    let mut stack_size = usize::MAX;
    let mut min_size = usize::MAX;
    let mut max_size = usize::MAX;
    let mut keep_alive_time = u64::MAX;
    let mut hook = true;
    if !args.is_empty() {
        let tea_parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("event_loop_size") {
                event_loop_size = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("stack_size") {
                stack_size = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("min_size") {
                min_size = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("max_size") {
                max_size = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("keep_alive_time") {
                keep_alive_time = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("hook") {
                hook = meta.value()?.parse::<LitBool>()?.value();
            }
            Ok(())
        });
        parse_macro_input!(args with tea_parser);
    }

    let func = parse_macro_input!(func as ItemFn);
    let func_vis = &func.vis; // like pub
    let func_block = &func.block; // { some statement or expression here }

    let func_decl = func.sig;
    let func_name = &func_decl.ident; // function name
    let func_generics = &func_decl.generics;
    let func_inputs = &func_decl.inputs;
    let func_output = &func_decl.output;

    let caller = quote! {
        // rebuild the function, add a func named is_expired to check user login session expire or not.
        #func_vis fn #func_name #func_generics(#func_inputs) #func_output {
            let mut open_coroutine_config = open_coroutine::Config::default();
            if #event_loop_size != usize::MAX {
                open_coroutine_config.set_event_loop_size(#event_loop_size);
            }
            if #stack_size != usize::MAX {
                open_coroutine_config.set_stack_size(#stack_size);
            }
            if #min_size != usize::MAX {
                open_coroutine_config.set_min_size(#min_size);
            }
            if #max_size != usize::MAX {
                open_coroutine_config.set_max_size(#max_size);
            }
            if #keep_alive_time != u64::MAX {
                open_coroutine_config.set_keep_alive_time(#keep_alive_time);
            }
            if #hook != true {
                open_coroutine_config.set_hook(#hook);
            }
            open_coroutine::init(open_coroutine_config);
            let _open_coroutine_result = #func_block;
            open_coroutine::shutdown();
            _open_coroutine_result
        }
    };
    caller.into()
}
