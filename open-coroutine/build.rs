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
    let dylib = env::current_dir().unwrap().join("dylib");
    #[cfg(target_os = "linux")]
    std::fs::copy(dylib.join("libhook.so"), deps.join("libhook.so"))
        .expect("copy libhook.so failed!");
    #[cfg(target_os = "macos")]
    std::fs::copy(dylib.join("libhook.dylib"), deps.join("libhook.dylib"))
        .expect("copy libhook.dylib failed!");
    #[cfg(target_os = "windows")]
    std::fs::copy(dylib.join("libhook.dll"), deps.join("libhook.dll"))
        .expect("copy libhook.dll failed!");
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=hook");
}
