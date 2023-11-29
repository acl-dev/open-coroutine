fn main() {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            cc::Build::new()
                .warnings(true)
                .file("c_src/version.c")
                .compile("version");
        }
    }
}
