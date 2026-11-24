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
        achievements: app.config.join("retroachievements.token"),
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

/// Loads SDL3 and Mesa's libraries into the process ahead of SDL, which then finds them already
/// there. Cold, from the SD card, they take ~0.5-1 s even without LLVM. RTLD_LAZY avoids eagerly
/// resolving thousands of unused desktop-GL / other-Mali symbols in libgallium at startup.
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
            "libSDL3.so.0",
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
            unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
        }
    }
}

/// Tells systemd (Type=notify) that the shelf's first frame is on screen, so deferred services
/// (nibos-cpufreq, late modules, zram, growfs, bootlog) can start now that boot UI start-up is
/// done and the CPU can leave the performance governor.
pub fn first_frame_shown() {
    #[cfg(target_os = "linux")]
    sd_notify_ready();
}

#[cfg(target_os = "linux")]
fn sd_notify_ready() {
    use std::os::unix::ffi::OsStrExt;
    let Some(sock) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    let bytes = sock.as_bytes();
    if bytes.is_empty() {
        return;
    }
    // SAFETY: AF_UNIX datagram socket for systemd's notification socket.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return;
    }
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let max = addr.sun_path.len();
    if bytes.len() <= max {
        // Abstract sockets start with '@' in NOTIFY_SOCKET and '\0' in sun_path.
        for (i, &b) in bytes.iter().enumerate() {
            addr.sun_path[i] = if i == 0 && b == b'@' { 0 } else { b as _ };
        }
        let len =
            (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len()) as libc::socklen_t;
        let msg = b"READY=1\n";
        // SAFETY: valid buffer, socket fd, and sockaddr_un of length `len`.
        unsafe {
            libc::sendto(
                fd,
                msg.as_ptr().cast(),
                msg.len(),
                libc::MSG_NOSIGNAL,
                (&addr as *const libc::sockaddr_un).cast(),
                len,
            );
        }
    }
    // SAFETY: our own descriptor.
    unsafe { libc::close(fd) };
}

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
