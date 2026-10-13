use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    if target.starts_with("aarch64-unknown-linux") {
        // Link against the device's own libSDL2 (pulled from the stock image).
        println!("cargo:rustc-link-search=native={dir}/sysroot");
        println!("cargo:rustc-link-arg=-Wl,--allow-shlib-undefined");
    } else if target.contains("apple") {
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    }
}
