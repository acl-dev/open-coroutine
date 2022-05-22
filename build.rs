extern crate cc;
extern crate bindgen;

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=lib_fiber/c/include/fiber/libfiber.h");
    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("lib_fiber/c/include/fiber/libfiber.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings")
        // Write the bindings to the src/bindings.rs file.
        .write_to_file("src/fiber.rs")
        .expect("Couldn't write bindings!");

    //if libfiber.a not exists, we need to build it before execute
    Command::new("make")
        .current_dir("lib_fiber/c")
        .status()
        .expect("process failed to execute");
    println!("cargo:rustc-link-search=native=lib_fiber/lib");
    println!("cargo:rustc-link-lib=dylib=fiber");
}