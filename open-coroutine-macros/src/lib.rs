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

#[macro_use]
extern crate quote;
#[macro_use]
extern crate syn;

use proc_macro::TokenStream;
use syn::{ItemFn, LitInt};

#[proc_macro_attribute]
pub fn main(args: TokenStream, func: TokenStream) -> TokenStream {
    let mut event_loop_size = num_cpus::get();
    let mut stack_size = 64 * 1024usize;
    let mut min_size = 0usize;
    let mut max_size = 65536usize;
    let mut keep_alive_time = 0u64;
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
            let open_coroutine_config = open_coroutine::Config::default();
            open_coroutine_config.set_event_loop_size(#event_loop_size)
                    .set_stack_size(#stack_size)
                    .set_min_size(#min_size)
                    .set_max_size(#max_size)
                    .set_keep_alive_time(#keep_alive_time);
            open_coroutine::init(open_coroutine_config);
            let _open_coroutine_result = #func_block;
            open_coroutine::shutdown();
            _open_coroutine_result
        }
    };
    caller.into()
}
