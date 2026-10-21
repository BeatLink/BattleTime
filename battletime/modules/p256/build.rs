// Lets the linker find module.x, which sits beside the workspace's manifest.
fn main() {
    let modules = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    println!("cargo:rustc-link-search={}", modules.display());
    println!("cargo:rerun-if-changed=../module.x");
}
