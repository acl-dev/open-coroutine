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
    let native = env::current_dir().unwrap().join("native");
    if cfg!(target_os = "linux") {
        let target = deps.join("libhook.so");
        if !target
            .try_exists()
            .expect("Can't check existence of file libhook.so")
        {
            std::fs::copy(native.join("libhook.so"), target).expect("copy libhook.so failed!");
        }
    } else if cfg!(target_os = "macos") {
        let target = deps.join("libhook.dylib");
        if !target
            .try_exists()
            .expect("Can't check existence of file libhook.dylib")
        {
            std::fs::copy(native.join("libhook.dylib"), target)
                .expect("copy libhook.dylib failed!");
        }
    } else if cfg!(target_os = "windows") {
        let target = deps.join("libhook.dll");
        if !target
            .try_exists()
            .expect("Can't check existence of file libhook.dll")
        {
            std::fs::copy(native.join("libhook.dll"), target).expect("copy libhook.dll failed!");
        }
    } else {
        panic!("unsupported platform!");
    }
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=hook");
}
