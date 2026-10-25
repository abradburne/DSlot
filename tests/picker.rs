//! The system picker opened from every page, with a window for each screen as on nibOS: a
//! texture made for one screen and drawn on the other fails there ("Texture was not created
//! with this renderer"), which a single preview window never shows.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder of its own under the target directory, emptied first.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// DSlot run headless on the demo library with `keys` pressed, one a frame: its log.
fn run(name: &str, keys: &[&str]) -> String {
    let dir = scratch(name);
    let (roms, home) = (dir.join("Roms"), dir.join("home"));
    let dslot = env!("CARGO_BIN_EXE_dslot");
    let made = Command::new(dslot)
        .arg("--make-demo")
        .arg(&roms)
        .status()
        .unwrap();
    assert!(made.success());
    let out = Command::new(dslot)
        .env("SDL_VIDEODRIVER", "dummy")
        .env("DSLOT_TWO_WINDOWS", "1")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .arg("--roms")
        .arg(&roms)
        .arg("--press")
        .arg(keys.join(","))
        .arg("--shot")
        .arg(dir.join("shot.png"))
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "dslot failed:\n{log}");
    log
}

/// Frames with nothing pressed, for the artwork to come in.
const WAIT: [&str; 20] = ["wait"; 20];

#[test]
fn picker_from_every_page_on_two_windows() {
    let mut keys = Vec::new();
    // Browse the pages on the top screen first, so that their artwork is made, then open the
    // picker on each page in turn and run through every blade both ways.
    for _ in 0..6 {
        keys.extend(WAIT);
        keys.push("r");
        keys.extend(WAIT);
        keys.push("s");
        keys.extend(WAIT);
        keys.extend(["Down"; 8]);
        keys.extend(WAIT);
        keys.extend(["Up"; 8]);
        keys.push("s");
    }
    let log = run("picker-two-windows", &keys);
    assert!(!log.contains("not drawn"), "{log}");
    assert!(!log.contains("not created with this renderer"), "{log}");
}
