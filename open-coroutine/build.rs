use std::env;
use std::path::PathBuf;

fn main() {
    //fix dylib name
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let deps = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("deps");
    let mut pattern = deps.to_str().unwrap().to_owned();
    if cfg!(target_os = "linux") {
        pattern += "/libopen_coroutine_hooks*.so";
        for path in glob::glob(&pattern)
            .expect("Failed to read glob pattern")
            .flatten()
        {
            std::fs::rename(path, deps.join("libopen_coroutine_hooks.so"))
                .expect("rename to libopen_coroutine_hooks.so failed!");
        }
    } else if cfg!(target_os = "macos") {
        pattern += "/libopen_coroutine_hooks*.dylib";
        for path in glob::glob(&pattern)
            .expect("Failed to read glob pattern")
            .flatten()
        {
            std::fs::rename(path, deps.join("libopen_coroutine_hooks.dylib"))
                .expect("rename to libopen_coroutine_hooks.dylib failed!");
        }
    } else if cfg!(target_os = "windows") {
        let dll_pattern = pattern.clone() + "/open_coroutine_hooks*.dll";
        for path in glob::glob(&dll_pattern)
            .expect("Failed to read glob pattern")
            .flatten()
        {
            std::fs::rename(path, deps.join("open_coroutine_hooks.dll"))
                .expect("rename to open_coroutine_hooks.dll failed!");
        }

        let lib_pattern = pattern + "/open_coroutine_hooks*.lib";
        for path in glob::glob(&lib_pattern)
            .expect("Failed to read glob pattern")
            .flatten()
        {
            std::fs::rename(path, deps.join("open_coroutine_hooks.lib"))
                .expect("rename to open_coroutine_hooks.lib failed!");
        }
    } else {
        panic!("unsupported platform");
    }
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=open_coroutine_hooks");
}
