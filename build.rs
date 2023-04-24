use std::env;
use std::path::PathBuf;

fn main() {
    //copy dylib to deps
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let deps = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("deps");
    let lib = env::current_dir().unwrap().join("lib");
    if cfg!(target_os = "linux") {
        std::fs::copy(
            lib.join("libopen_coroutine_hooks.so"),
            deps.join("libopen_coroutine_hooks.so"),
        )
        .expect("copy libopen_coroutine_hooks.so failed!");
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            std::fs::copy(
                lib.join("libopen_coroutine_hooks-m1.dylib"),
                deps.join("libopen_coroutine_hooks.dylib"),
            )
            .expect("copy libopen_coroutine_hooks-m1.dylib failed!");
        } else {
            std::fs::copy(
                lib.join("libopen_coroutine_hooks.dylib"),
                deps.join("libopen_coroutine_hooks.dylib"),
            )
            .expect("copy libopen_coroutine_hooks.dylib failed!");
        }
    } else if cfg!(target_os = "windows") {
        std::fs::copy(lib.join("hook.dll"), deps.join("hook.dll")).expect("copy hook.dll failed!");
        std::fs::copy(lib.join("hook.dll.lib"), deps.join("hook.lib"))
            .expect("copy hook.lib failed!");
    }
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=open_coroutine_hooks");
}
