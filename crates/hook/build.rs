fn main() {
    let def =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("hook.def");
    println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
    println!("cargo:rerun-if-changed=hook.def");
}
