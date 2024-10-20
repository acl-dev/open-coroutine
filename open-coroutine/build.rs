use std::env::var;
use std::fs::{read_dir, rename};
use std::path::PathBuf;

fn main() {
    // build dylib
    let target = var("TARGET").expect("env not found");
    let out_dir = PathBuf::from(var("OUT_DIR").expect("env not found"));
    let cargo_manifest_dir = PathBuf::from(var("CARGO_MANIFEST_DIR").expect("env not found"));
    let mut cargo = std::process::Command::new("cargo");
    let mut cmd = cargo.arg("build").arg("--target").arg(target.clone());
    if cfg!(not(debug_assertions)) {
        cmd = cmd.arg("--release");
    }
    if let Err(e) = cmd
        .arg("--manifest-path")
        .arg(
            cargo_manifest_dir
                .parent()
                .expect("parent not found")
                .join("hook")
                .join("Cargo.toml"),
        )
        .arg("--target-dir")
        .arg(out_dir.clone())
        .status()
    {
        panic!("failed to build build dylib {e}");
    }
    //fix dylib name
    let hook_deps = out_dir
        .join(target)
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join("deps");
    let deps = out_dir
        .parent()
        .expect("can not find deps dir")
        .parent()
        .expect("can not find deps dir")
        .parent()
        .expect("can not find deps dir")
        .join("deps");
    let lib_names = [
        String::from("libopen_coroutine_hook.so"),
        String::from("libopen_coroutine_hook.dylib"),
        String::from("open_coroutine_hook.lib"),
    ];
    for entry in read_dir(hook_deps.clone())
        .expect("Failed to read deps")
        .flatten()
    {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.contains("open_coroutine_hook") {
            continue;
        }
        if lib_names.contains(&file_name) {
            break;
        }
        if file_name.eq("open_coroutine_hook.dll") {
            continue;
        }
        if cfg!(target_os = "linux") && file_name.ends_with(".so") {
            rename(
                hook_deps.join(file_name),
                deps.join("libopen_coroutine_hook.so"),
            )
            .expect("rename to libopen_coroutine_hook.so failed!");
        } else if cfg!(target_os = "macos") && file_name.ends_with(".dylib") {
            rename(
                hook_deps.join(file_name),
                deps.join("libopen_coroutine_hook.dylib"),
            )
            .expect("rename to libopen_coroutine_hook.dylib failed!");
        } else if cfg!(windows) {
            if file_name.ends_with(".dll") {
                rename(
                    hook_deps.join(file_name),
                    deps.join("open_coroutine_hook.dll"),
                )
                .expect("rename to open_coroutine_hook.dll failed!");
            } else if file_name.ends_with(".lib") {
                rename(
                    hook_deps.join(file_name),
                    deps.join("open_coroutine_hook.lib"),
                )
                .expect("rename to open_coroutine_hook.lib failed!");
            }
        }
    }
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=open_coroutine_hook");
}
