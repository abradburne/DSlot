//! nibOS (mainline Linux and Arch Linux ARM on the RG DS+): DSlot is the boot UI, started by
//! `dslot.service` in place of the tty1 login shell, and packaged with packaging/nibos/PKGBUILD.
//! Its files are the user's, in the usual XDG places; emulators install their descriptions in
//! /usr/share/dslot/emulators.

use super::Dirs;
use crate::settings::Legacy;
use nibkit::paths::AppDirs;
use std::path::{Path, PathBuf};

/// On the ROMS partition, whichever card the game is on, where the stock OS's apps also look.
pub fn saves() -> Option<PathBuf> {
    Some(nibkit::paths::saves_root())
}

/// Left by nibOS's power-on splash (nibos/rootfs/splash) once its picture is on screen.
pub const SPLASH_SHOWN: &str = "/run/nibos/splash";

/// The pictures nibOS's splash shows, the first that reads (nibos/rootfs/splash's PICTURES).
pub const SPLASH_PICTURES: &[&str] =
    &["/usr/lib/nibos/pictures/boot.pspl", "/var/lib/dslot/splash"];

pub fn dirs() -> Dirs {
    let app = AppDirs::new("dslot");
    Dirs {
        settings: app.config.join("settings.toml"),
        legacy: Legacy {
            settings_ini: Some(app.config.join("settings.ini")),
            launch_conf: Some(app.config.join("launch.conf")),
        },
        stats: app.data.join("stats.tsv"),
        cache: app.cache,
        // Packages', then the administrator's, then the user's own.
        emulators: nibkit::dslot::description_dirs(),
    }
}

/// Arch Linux ARM's Noto CJK (noto-fonts-cjk), when installed.
pub const FALLBACK_FONTS: &[&str] = &[
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJKsc-Regular.otf",
];

/// Menu is the button that brings you back from a game; a press that arrives as the shelf
/// comes back must not leave it as well.
pub const MENU_QUITS: bool = false;

/// The shelf's layout straight through KMS first, then the shelf on GL (DSLOT_KMS).
pub const SKELETON_START: bool = true;

/// nibOS's splash has the DSlot logo on screen already: no skeleton wanted, SDL takes the
/// outputs straight over.
pub fn splash_shown() -> bool {
    Path::new(SPLASH_SHOWN).exists()
}

/// Loads Mesa's libraries into the process ahead of SDL, which then finds them already there.
/// Cold, from the SD card, they take ~0.5-1 s even without LLVM.
pub fn preload_gl() {
    #[cfg(target_os = "linux")]
    {
        let gallium = std::fs::read_dir("/usr/lib")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("libgallium-"))
            });
        let libs = [
            "libgbm.so.1",
            "libEGL.so.1",
            "libEGL_mesa.so.0",
            "libGLESv2.so.2",
        ]
        .map(PathBuf::from);
        for lib in libs.iter().chain(gallium.iter()) {
            let Ok(name) = std::ffi::CString::new(lib.as_os_str().to_string_lossy().as_bytes())
            else {
                continue;
            };
            // SAFETY: a NUL-terminated path; a library that is missing just returns null.
            unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
        }
    }
}

pub fn first_frame_shown() {}

/// Started by systemd as root, dslot.service has no drop-in naming its user: DSlot's files,
/// and the games' settings and saves, would all be root's.
pub fn check_user() -> Result<(), String> {
    // SAFETY: getuid has no preconditions.
    let root = unsafe { libc::getuid() } == 0;
    if root && std::env::var_os("INVOCATION_ID").is_some() {
        return Err(
            "not starting as root: dslot.service needs a User= (dslot.service.d/user.conf)".into(),
        );
    }
    Ok(())
}

/// Starting at power-on is nibOS's switch, `sudo nib boot-ui on|off`, which needs root: the
/// menu does not offer it.
pub fn start_at_power_on() -> Option<bool> {
    None
}

pub fn set_start_at_power_on(_on: bool) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "switched with `sudo nib boot-ui on|off` on nibOS",
    ))
}

pub fn refresh_power_on_hook() {}

/// Where to read DSlot's whole log, as the shelf's notices say.
pub const LOG_HINT: &str = "journalctl -u dslot";
