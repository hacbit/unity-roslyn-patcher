fn main() {
    assert_eq!(
        std::env::var("TARGET").unwrap(),
        "x86_64-pc-windows-msvc",
        "Windows x64 MSVC only"
    );
    let root = "../../vendor/Detours-4.0.1/src";
    let mut b = cc::Build::new();
    b.cpp(true)
        .include(root)
        .define("WIN32_LEAN_AND_MEAN", None)
        .warnings(false);
    for file in [
        "detours",
        "modules",
        "disasm",
        "image",
        "creatwth",
        "disolx86",
        "disolx64",
        "disolia64",
        "disolarm",
        "disolarm64",
    ] {
        b.file(format!("{root}/{file}.cpp"));
    }
    b.compile("detours");
    println!("cargo:rerun-if-changed={root}");
}
