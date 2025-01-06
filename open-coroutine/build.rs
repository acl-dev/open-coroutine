use cargo_metadata::MetadataCommand;
use std::env::var;
use std::fs::{copy, read_dir};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

fn main() {
    // init log
    let out_dir = PathBuf::from(var("OUT_DIR").expect("OUT_DIR not found"));
    let target_dir = out_dir
        .parent()
        .expect("can not find deps dir")
        .parent()
        .expect("can not find deps dir")
        .parent()
        .expect("can not find deps dir")
        .parent()
        .expect("can not find deps dir");
    _ = tracing_subscriber::fmt()
        .with_writer(RollingFileAppender::new(
            Rotation::NEVER,
            target_dir,
            "open-coroutine-build.log",
        ))
        .with_thread_names(true)
        .with_line_number(true)
        .with_max_level(Level::INFO)
        .with_timer(tracing_subscriber::fmt::time::OffsetTime::new(
            time::UtcOffset::from_hms(8, 0, 0).expect("create UtcOffset failed !"),
            time::format_description::well_known::Rfc2822,
        ))
        .try_init();
    // build dylib
    let target = var("TARGET").expect("env not found");
    let mut cargo = std::process::Command::new("cargo");
    let mut cmd = cargo.arg("build").arg("--target").arg(target.clone());
    if cfg!(not(debug_assertions)) {
        cmd = cmd.arg("--release");
    }
    let mut hook_toml = PathBuf::from(var("CARGO_MANIFEST_DIR").expect("env not found"))
        .parent()
        .expect("parent not found")
        .join("hook")
        .join("Cargo.toml");
    let metadata = MetadataCommand::default()
        .no_deps()
        .exec()
        .expect("read cargo metadata failed");
    let package = if hook_toml.exists() {
        metadata
            .packages
            .iter()
            .find(|pkg| pkg.name.eq("open-coroutine"))
            .expect("read current package failed")
    } else {
        metadata
            .packages
            .first()
            .expect("read current package failed")
    };
    info!("read package:{:#?}", package);
    let dependency = package
        .dependencies
        .iter()
        .find(|dep| dep.name.eq("open-coroutine-hook"))
        .expect("open-coroutine-hook not found");
    if !hook_toml.exists() {
        info!(
            "{:?} not exists, find open-coroutine-hook's Cargo.toml in $CARGO_HOME",
            hook_toml
        );
        // 使用cargo_metadata读到依赖版本，结合CARGO_HOME获取open-coroutine-hook的toml
        let dep_src_dir = PathBuf::from(var("CARGO_HOME").expect("CARGO_HOME not found"))
            .join("registry")
            .join("src");
        let crates_parent_dirs = Vec::from_iter(
            read_dir(dep_src_dir.clone())
                .expect("Failed to read deps")
                .flatten(),
        );
        let crates_parent = if crates_parent_dirs.len() == 1 {
            crates_parent_dirs.first().expect("host dir not found")
        } else {
            let rustup_dist_server =
                var("RUSTUP_DIST_SERVER").expect("RUSTUP_DIST_SERVER not found");
            let host = rustup_dist_server
                .split("://")
                .last()
                .expect("host not found");
            crates_parent_dirs
                .iter()
                .find(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .to_string()
                        .contains(host)
                })
                .unwrap_or_else(|| {
                    crates_parent_dirs
                        .iter()
                        .find(|entry| {
                            entry
                                .file_name()
                                .to_string_lossy()
                                .to_string()
                                .contains("crates.io")
                        })
                        .expect("host dir not found")
                })
        }
        .file_name()
        .to_string_lossy()
        .to_string();
        info!("crates parent dirs:{:?}", crates_parent_dirs);
        let version = &dependency
            .req
            .comparators
            .first()
            .expect("version not found");
        hook_toml = dep_src_dir
            .join(crates_parent)
            .join(format!(
                "open-coroutine-hook-{}.{}.{}",
                version.major,
                version.minor.unwrap_or(0),
                version.patch.unwrap_or(0)
            ))
            .join("Cargo.toml");
    }
    info!("open-coroutine-hook's Cargo.toml is here:{:?}", hook_toml);
    if !dependency.uses_default_features {
        cmd = cmd.arg("--no-default-features");
    }
    let mut features = Vec::new();
    if cfg!(feature = "log") {
        features.push("log");
    }
    if cfg!(feature = "preemptive") {
        features.push("preemptive");
    }
    if cfg!(feature = "net") {
        features.push("net");
    }
    if cfg!(feature = "io_uring") {
        features.push("io_uring");
    }
    if cfg!(feature = "iocp") {
        features.push("iocp");
    }
    if cfg!(feature = "completion_io") {
        features.push("completion_io");
    }
    if cfg!(feature = "syscall") {
        features.push("syscall");
    }
    info!(
        "use open-coroutine-hook's default-features:{} and features:{:?}",
        dependency.uses_default_features, features
    );
    if let Err(e) = cmd
        .arg("--features")
        .arg(features.join(","))
        .arg("--manifest-path")
        .arg(hook_toml)
        .arg("--target-dir")
        .arg(out_dir.clone())
        .status()
    {
        panic!("failed to build dylib {}", e);
    }
    // correct dylib path
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
    for entry in read_dir(hook_deps.clone())
        .expect("can not find deps dir")
        .flatten()
    {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.contains("open_coroutine_hook") {
            continue;
        }
        if cfg!(target_os = "linux") && file_name.ends_with(".so") {
            let from = hook_deps.join(file_name);
            let to = deps.join("libopen_coroutine_hook.so");
            copy(from.clone(), to.clone()).expect("copy to libopen_coroutine_hook.so failed!");
            info!("copy {:?} to {:?} success!", from, to);
        } else if cfg!(target_os = "macos") && file_name.ends_with(".dylib") {
            let from = hook_deps.join(file_name);
            let to = deps.join("libopen_coroutine_hook.dylib");
            copy(from.clone(), to.clone()).expect("copy to libopen_coroutine_hook.dylib failed!");
            info!("copy {:?} to {:?} success!", from, to);
        } else if cfg!(windows) {
            if file_name.ends_with(".dll") {
                let from = hook_deps.join(file_name);
                let to = deps.join("open_coroutine_hook.dll");
                copy(from.clone(), to.clone()).expect("copy to open_coroutine_hook.dll failed!");
                info!("copy {:?} to {:?} success!", from, to);
            } else if file_name.ends_with(".lib") {
                let from = hook_deps.join(file_name);
                let to = deps.join("open_coroutine_hook.lib");
                copy(from.clone(), to.clone()).expect("copy to open_coroutine_hook.lib failed!");
                info!("copy {:?} to {:?} success!", from, to);
            }
        }
    }
    // link dylib
    println!("cargo:rustc-link-search=native={}", deps.display());
    println!("cargo:rustc-link-lib=dylib=open_coroutine_hook");
}
