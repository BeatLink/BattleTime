//! Puts memory.x where the linker finds it.

fn main() {
    let directory = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={directory}");
    println!("cargo:rerun-if-changed=memory.x");
}
