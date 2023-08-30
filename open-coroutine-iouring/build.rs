fn main() {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            cc::Build::new()
                .cpp(true)
                .warnings(true)
                .flag("-Wall")
                .flag("-std=c++11")
                .flag("-c")
                .file("cpp_src/version.cpp")
                .compile("version");
        }
    }
}
