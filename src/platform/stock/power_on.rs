//! Starting at power-on. Before its own menu comes up, the stock boot script
//! (`/mnt/mod/ctrl/autostart`) runs `update/update.sh` from either card if it is there. Turning
//! this on copies DSlot's `boot.sh` to that place on the card DSlot lives on; nothing on the
//! system partitions is touched, and deleting the file on a computer undoes it.
//!
//! On the card the OS starts from, a second script goes beside it, `update/early.sh`. The
//! stock system never runs it. dsboot, if installed, runs it as soon as the screen is up,
//! about a second before the stock boot script reaches `update.sh`, which then only waits.
//! Both scripts are in packaging/stock-card/Roms/APPS/dslot/.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The stock script that runs the hook. Without it there is no way to start at power-on.
const RUNNER: &str = "/mnt/mod/ctrl/autostart";
/// Where DSlot 0.1 put its hook. The stock menu ignores it unless its default style is in use.
const OLD_HOOK: &str = "/mnt/mmc/dmenu.bin";
/// Where the card the OS starts from is mounted.
const EARLY_CARD: &str = "/mnt/mmc";

/// DSlot's hook script, `<card>/Roms/APPS/dslot/boot.sh`.
fn script() -> PathBuf {
    super::exe_dir().join("boot.sh")
}

/// The hook's place, given our script in `<card>/Roms/APPS/dslot/`.
fn hook(script: &Path) -> Option<PathBuf> {
    Some(script.ancestors().nth(4)?.join("update/update.sh"))
}

/// The early script's place, where there is one: only the card the OS starts from is
/// mounted early enough for it to be of use.
fn early_hook(script: &Path) -> Option<PathBuf> {
    let card = script.ancestors().nth(4)?;
    (card == Path::new(EARLY_CARD)).then(|| card.join("update/early.sh"))
}

/// Our early script, beside `script`.
fn early(script: &Path) -> PathBuf {
    script.with_file_name("early.sh")
}

/// A hook file is ours if it mentions DSlot; anything else there is left alone.
fn ours(file: &Path) -> bool {
    fs::read_to_string(file).is_ok_and(|s| s.contains("dslot"))
}

pub fn available() -> bool {
    Path::new(RUNNER).is_file()
}

pub fn enabled() -> bool {
    hook(&script()).is_some_and(|h| ours(&h))
}

/// Install or remove the hook. A hook that is someone else's is left alone.
pub fn set(on: bool) -> io::Result<()> {
    let script = script();
    let hook = hook(&script).ok_or_else(|| io::Error::other("no card to install on"))?;
    if hook.exists() && !ours(&hook) {
        return Err(io::Error::other(format!(
            "{} belongs to something else",
            hook.display()
        )));
    }
    if ours(Path::new(OLD_HOOK)) {
        fs::remove_file(OLD_HOOK)?;
    }
    let folder = hook.parent().unwrap_or(Path::new(""));
    let early_hook = early_hook(&script).filter(|h| !h.exists() || ours(h));
    if on {
        fs::create_dir_all(folder)?;
        if let Some(early_hook) = early_hook {
            fs::copy(early(&script), early_hook)?;
        }
        fs::copy(&script, &hook).map(|_| ())
    } else if hook.exists() {
        if let Some(early_hook) = early_hook.filter(|h| h.exists()) {
            fs::remove_file(early_hook)?;
        }
        fs::remove_file(&hook)?;
        // Leave no empty folder behind; one with other things in it stays.
        let _ = fs::remove_dir(folder);
        Ok(())
    } else {
        Ok(())
    }
}

/// Bring an installed hook up to date with the scripts beside DSlot, which a new version may
/// have changed, and move one from the old place to the new.
pub fn refresh() {
    let script = script();
    let Some(hook) = hook(&script) else { return };
    let differs = |ours: &Path, installed: &Path| {
        fs::read(ours).is_ok_and(|new| fs::read(installed).map_or(true, |old| old != new))
    };
    let early_stale = early_hook(&script)
        .is_some_and(|h| (!h.exists() || ours(&h)) && differs(&early(&script), &h));
    let stale = ours(&hook) && (differs(&script, &hook) || early_stale);
    if available() && (stale || ours(Path::new(OLD_HOOK))) {
        match set(true) {
            Ok(()) => eprintln!("dslot: start at power-on: hook updated"),
            Err(e) => eprintln!("dslot: start at power-on: cannot update hook: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_places() {
        let on_os_card = Path::new("/mnt/mmc/Roms/APPS/dslot/boot.sh");
        assert_eq!(
            hook(on_os_card),
            Some(PathBuf::from("/mnt/mmc/update/update.sh"))
        );
        assert_eq!(
            early_hook(on_os_card),
            Some(PathBuf::from("/mnt/mmc/update/early.sh"))
        );
        assert_eq!(
            early(on_os_card),
            Path::new("/mnt/mmc/Roms/APPS/dslot/early.sh")
        );

        // The other card is mounted too late for the early script to be of use.
        let on_games_card = Path::new("/mnt/sdcard/Roms/APPS/dslot/boot.sh");
        assert_eq!(
            hook(on_games_card),
            Some(PathBuf::from("/mnt/sdcard/update/update.sh"))
        );
        assert_eq!(early_hook(on_games_card), None);

        assert_eq!(hook(Path::new("/boot.sh")), None);
    }
}
