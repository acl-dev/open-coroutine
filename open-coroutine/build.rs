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
            .find(|pkg| pkg.name.as_str().eq("open-coroutine"))
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
        } else if let Ok(rustup_dist_server) = var("RUSTUP_DIST_SERVER") {
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
        } else {
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
    let status = cmd
        .arg("--features")
        .arg(features.join(","))
        .arg("--manifest-path")
        .arg(hook_toml)
        .arg("--target-dir")
        .arg(out_dir.clone())
        .status()
        .expect("failed to build dylib");
    if !status.success() {
        panic!("failed to build dylib, status: {status}");
    }
    // correct dylib path
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let nested_profile = out_dir.join(&target).join(profile);
    // Cargo >= 1.100 uses the new build-dir layout: cdylib final artifacts are
    // uplifted to the profile root and per-unit artifacts live under
    // `build/<pkg>/<hash>/out`, while older cargos keep everything in `deps/`.
    let mut search_dirs = vec![nested_profile.clone(), nested_profile.join("deps")];
    let hook_units = nested_profile.join("build").join("open-coroutine-hook");
    if let Ok(entries) = read_dir(&hook_units) {
        for entry in entries.flatten() {
            let unit_out = entry.path().join("out");
            if unit_out.is_dir() {
                search_dirs.push(unit_out);
            }
        }
    }
    search_dirs.sort();
    search_dirs.dedup();
    // The main target profile dir is the parent of the `build` dir that
    // contains this build script's OUT_DIR, in both old and new layouts.
    let profile_dir = out_dir
        .ancestors()
        .find(|p| p.file_name() == Some(std::ffi::OsStr::new("build")))
        .and_then(|p| p.parent())
        .expect("can not find profile dir")
        .to_path_buf();
    // `deps` holds executables in the old layout; the profile root holds
    // uplifted executables in the new layout. Copy into both.
    let deps = profile_dir.join("deps");
    let dest_dirs = [profile_dir, deps];
    for dest in &dest_dirs {
        std::fs::create_dir_all(dest).expect("can not create deps dir");
    }
    let mut found = false;
    for dir in &search_dirs {
        let Ok(entries) = read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.contains("open_coroutine_hook") {
                continue;
            }
            let canonical = if cfg!(target_os = "linux") && file_name.ends_with(".so") {
                "libopen_coroutine_hook.so"
            } else if cfg!(target_os = "macos") && file_name.ends_with(".dylib") {
                "libopen_coroutine_hook.dylib"
            } else if cfg!(windows) && file_name.ends_with(".dll") {
                "open_coroutine_hook.dll"
            } else if cfg!(windows) && file_name.ends_with(".lib") {
                // covers `open_coroutine_hook.dll.lib` and `open_coroutine_hook.lib`
                "open_coroutine_hook.lib"
            } else {
                continue;
            };
            let from = dir.join(&file_name);
            found = true;
            for dest in &dest_dirs {
                let to = dest.join(canonical);
                copy(from.clone(), to.clone())
                    .unwrap_or_else(|e| panic!("copy {from:?} to {to:?} failed: {e}"));
                info!("copy {from:?} to {to:?} success!");
            }
        }
    }
    if !found {
        panic!("open_coroutine_hook artifact not found in {search_dirs:?}");
    }
    // link dylib
    for dest in &dest_dirs {
        println!("cargo:rustc-link-search=native={}", dest.display());
    }
    println!("cargo:rustc-link-lib=dylib=open_coroutine_hook");
}
