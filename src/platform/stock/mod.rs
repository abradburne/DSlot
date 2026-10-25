//! The stock Anbernic OS (Buildroot, Weston, PulseAudio). DSlot is an app on a games card,
//! `Roms/APPS/DSlot.sh` and the folder `Roms/APPS/dslot/` beside it, and keeps all its files
//! in that folder. Games start in the emulators the system comes with, or any other with a
//! description in `dslot/emulators/` (packaging/stock-card).

mod power_on;

pub use power_on::{refresh as refresh_power_on_hook, set as set_start_at_power_on};

use super::Dirs;
use crate::settings::Legacy;
use std::path::PathBuf;

/// The stock emulators keep their saves where they always have.
pub fn saves() -> Option<PathBuf> {
    None
}

pub fn dirs() -> Dirs {
    let here = exe_dir();
    Dirs {
        settings: here.join("settings.toml"),
        legacy: Legacy {
            settings_ini: Some(here.join("settings.ini")),
            launch_conf: None,
        },
        stats: here.join("stats.tsv"),
        cache: here.join("cache"),
        emulators: vec![here.join("emulators")],
    }
}

/// CJK faces the system has.
pub const FALLBACK_FONTS: &[&str] = &[
    "/mnt/vendor/deep/nnddss/assets/fonts/SourceHanSansSC-Normal.ttf",
    "/mnt/vendor/bin/default.ttf",
];

/// Menu leaves DSlot for the stock menu, as it leaves the stock apps.
pub const MENU_QUITS: bool = true;

/// No skeleton: Weston has the screens, and is up long before DSlot.
pub const SKELETON_START: bool = false;

pub fn splash_shown() -> bool {
    false
}

/// No splash of nibOS's on the stock OS.
pub const SPLASH_PICTURES: &[&str] = &[];

pub fn preload_gl() {}

/// dsboot, when it started DSlot early, holds the radios back until the shelf is up, and
/// learns that from the file `DSBOOT_SHOWN` names.
pub fn first_frame_shown() {
    if let Ok(shown) = std::env::var("DSBOOT_SHOWN") {
        let _ = std::fs::write(shown, b"");
    }
}

/// The folder DSlot's binary is in: `Roms/APPS/dslot/`.
fn exe_dir() -> PathBuf {
    nibkit::paths::exe_dir()
}

/// Everything on the stock OS runs as root.
pub fn check_user() -> Result<(), String> {
    Ok(())
}

/// Whether DSlot starts at power-on, where that can be changed from its menu.
pub fn start_at_power_on() -> Option<bool> {
    power_on::available().then(power_on::enabled)
}
