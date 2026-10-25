//! Where the SDL2 library DSlot links against comes from, per platform.

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let stock = env::var_os("CARGO_FEATURE_STOCK").is_some();
    if stock && target.starts_with("aarch64-unknown-linux") {
        // Cross-built for the stock OS: linked against the handheld's own libSDL2, copied from
        // its image into sysroot/ (docs/building.md). Symbols it lacks are only an error if
        // DSlot calls them, which it does not.
        println!("cargo:rustc-link-search=native={dir}/sysroot");
        println!("cargo:rustc-link-arg=-Wl,--allow-shlib-undefined");
    } else if target.contains("apple") {
        // A desktop preview on a Mac finds Homebrew's SDL2. On nibOS, and in its build
        // machine, the system's SDL2 is linked as usual.
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
