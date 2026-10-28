//! What DSlot does differently on the two systems it runs on, chosen at build time by a Cargo
//! feature: `stock` (the Anbernic OS the RG DS+ ships with, where DSlot is an app on the games
//! card) or `nibos` (nibOS, where DSlot is the boot UI). The device's facts (where the screens
//! are, the buttons' labels, brightness, volume, where games are) are nibkit's, built for the
//! same system; what is DSlot's own policy is here.
//!
//! Each platform module provides the same items:
//!
//! - `SAVES`: where DSlot puts games' saves (`<SAVES>/<SYSTEM>/<ROM name>/`), if it does.
//! - `dirs()`: where DSlot keeps its settings, play times, cache and emulator descriptions.
//! - `FALLBACK_FONTS`: the system's CJK faces.
//! - `MENU_QUITS`: whether the Menu button leaves the shelf.
//! - `check_user`, `splash_shown`, `SKELETON_START`, `preload_gl`, `first_frame_shown`: start-up.
//! - `start_at_power_on`, `set_start_at_power_on`, `refresh_power_on_hook`: power-on start.

#[cfg(all(feature = "stock", feature = "nibos"))]
compile_error!("build for one platform: --features stock or --features nibos, not both");
#[cfg(not(any(feature = "stock", feature = "nibos")))]
compile_error!("build for a platform: --features stock or --features nibos (docs/building.md)");

#[cfg(feature = "nibos")]
mod nibos;
#[cfg(feature = "nibos")]
pub use nibos::*;

#[cfg(feature = "stock")]
mod stock;
#[cfg(feature = "stock")]
pub use stock::*;

use crate::settings::Legacy;
use nibkit::screens::Screen;
use nibkit::{backlight, device};
use std::path::PathBuf;

/// Where DSlot keeps its own files.
pub struct Dirs {
    /// `settings.toml`: the settings, at every level (settings/mod.rs).
    pub settings: PathBuf,
    /// What older versions kept instead, read into it once.
    pub legacy: Legacy,
    /// `stats.tsv`: play times, favourites and ratings.
    pub stats: PathBuf,
    /// `retroachievements.token`: the RetroAchievements sign-in, username and token
    /// (achievements/). Mode 0600; never in a log or a bug report.
    pub achievements: PathBuf,
    /// What each ROM said about itself, and grid covers already scaled. Safe to delete.
    pub cache: PathBuf,
    /// Folders of emulator descriptions, read in order: a file replaces one of the same name
    /// in an earlier folder.
    pub emulators: Vec<PathBuf>,
}

/// On the handheld, as against a desktop preview.
pub fn on_device() -> bool {
    nibkit::on_device()
}

/// The `Roms` folders looked in when none is given (`Roms/NDS`, `Roms/GBA`, ...).
pub fn rom_roots() -> Vec<PathBuf> {
    nibkit::paths::rom_roots()
}

/// Powers off, as the system does it, and stays up while it goes down so that nothing (the
/// stock menu, the console) flashes past.
pub fn power_off() {
    if on_device() {
        device::power_off();
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
}

/// Restarts, as the system does it, staying up while it goes down.
pub fn restart() {
    if on_device() {
        device::restart();
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
}

/// One of the device's own settings, which DSlot reads and changes as they are (not kept in
/// its settings file). Where the system keeps one to itself (the stock OS: all of them),
/// reading it gives `None` and the menu leaves it out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    /// The speaker's (or headphones') volume, in percent.
    Volume,
    /// A screen's brightness, in percent of its usable range.
    Brightness(Screen),
    /// Wi-Fi on or off (nibOS's `nib wifi`: Bluetooth is its own switch).
    Wifi,
    /// nibOS's battery protect: charging stops at 80% (and starts again at 75%).
    BatteryProtect,
}

pub const CONTROLS: &[Control] = &[
    Control::Volume,
    Control::Brightness(Screen::Top),
    Control::Brightness(Screen::Bottom),
    Control::Wifi,
    Control::BatteryProtect,
];

impl Control {
    pub fn label(self) -> &'static str {
        match self {
            Control::Volume => "Volume",
            Control::Brightness(Screen::Top) => "Top screen",
            Control::Brightness(Screen::Bottom) => "Bottom screen",
            Control::Wifi => "Wi-Fi",
            Control::BatteryProtect => "Battery protect",
        }
    }

    /// Whether it is on or off, as against a percentage.
    pub fn is_switch(self) -> bool {
        matches!(self, Control::Wifi | Control::BatteryProtect)
    }

    /// Whether reading it is a file read (sysfs), cheap enough to do whenever it may have
    /// changed; volume and Wi-Fi run a command.
    pub fn cheap(self) -> bool {
        matches!(self, Control::Brightness(_) | Control::BatteryProtect)
    }
}

/// A control's value now: a percentage, or 1 and 0 for on and off.
pub fn control(control: Control) -> Option<i64> {
    match control {
        Control::Volume => device::volume().map(i64::from),
        Control::Brightness(screen) => backlight::get(screen).map(i64::from),
        Control::Wifi => device::wifi().map(i64::from),
        Control::BatteryProtect => nibkit::battery::protect().map(i64::from),
    }
}

/// Changes a control at once: brightness and volume as they are adjusted, so the change is
/// seen and heard.
pub fn set_control(control: Control, value: i64) {
    let percent = value.clamp(0, 100) as u8;
    match control {
        Control::Volume => device::set_volume(percent),
        Control::Brightness(screen) => backlight::set(screen, percent),
        Control::Wifi => device::set_wifi(value != 0),
        // A command (`nib battery protect`): in the background, as Wi-Fi's.
        Control::BatteryProtect => {
            std::thread::spawn(move || {
                if let Err(e) = nibkit::battery::set_protect(value != 0) {
                    eprintln!("dslot: battery protect: {e}");
                }
            });
        }
    }
}

/// A control has been let go of: the screens' brightness is saved for the next boot.
pub fn control_settled(control: Control) {
    if let Control::Brightness(_) = control {
        backlight::save();
    }
}
