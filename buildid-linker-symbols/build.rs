use std::path::PathBuf;

fn main() {
    let n = "buildid-linker-symbols.lds";
    let mut s = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is required"),
    );
    s.push(n);
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR required"));
    std::fs::copy(s, out.join(n)).expect("failed to copy linker script to OUT_DIR");

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-T{}", n);

    println!("cargo:rerun-if-changed={}", n);
}
