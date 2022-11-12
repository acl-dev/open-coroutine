fn main() {
    //link hook dylib
    println!("cargo:rustc-link-lib=dylib=hook");
}
