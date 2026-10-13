//! Starting at power-on. Before its own menu comes up, the stock boot script
//! (`/mnt/mod/ctrl/autostart`) runs `update/update.sh` from either card if it is there. Turning
//! this on puts our script in that place on the card dslot lives on; nothing on the system
//! partitions is touched, and deleting the file on a computer undoes it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The stock script that runs the hook. Without it there is no way to start at power-on.
const RUNNER: &str = "/mnt/mod/ctrl/autostart";
/// Where 0.1 first put its hook. The stock menu ignores it unless its default style is in use.
const OLD_HOOK: &str = "/mnt/mmc/dmenu.bin";

/// The hook's place, given our script in `<card>/Roms/APPS/dslot/`.
fn hook(script: &Path) -> Option<PathBuf> {
    Some(script.ancestors().nth(4)?.join("update/update.sh"))
}

fn ours(file: &Path) -> bool {
    fs::read_to_string(file).is_ok_and(|s| s.contains("dslot"))
}

pub fn available() -> bool {
    Path::new(RUNNER).is_file()
}

pub fn enabled(script: &Path) -> bool {
    hook(script).is_some_and(|h| ours(&h))
}

/// Install or remove `script` as the hook. A hook that is someone else's is left alone.
pub fn set(on: bool, script: &Path) -> io::Result<()> {
    let hook = hook(script).ok_or_else(|| io::Error::other("no card to install on"))?;
    if hook.exists() && !ours(&hook) {
        return Err(io::Error::other(format!("{} belongs to something else", hook.display())));
    }
    if ours(Path::new(OLD_HOOK)) {
        fs::remove_file(OLD_HOOK)?;
    }
    if on {
        fs::create_dir_all(hook.parent().unwrap_or(Path::new("")))?;
        fs::copy(script, &hook).map(|_| ())
    } else if hook.exists() {
        fs::remove_file(&hook)?;
        // Leave no empty folder behind; one with other things in it stays.
        let _ = fs::remove_dir(hook.parent().unwrap_or(Path::new("")));
        Ok(())
    } else {
        Ok(())
    }
}

/// Bring an installed hook up to date with `script`, which a new version may have changed,
/// and move one in the old place to the new.
pub fn refresh(script: &Path) {
    let Some(hook) = hook(script) else { return };
    let stale = ours(&hook) && fs::read(script).is_ok_and(|new| fs::read(&hook).is_ok_and(|old| old != new));
    if available() && (stale || ours(Path::new(OLD_HOOK))) {
        match set(true, script) {
            Ok(()) => eprintln!("dslot: start at power-on: hook updated"),
            Err(e) => eprintln!("dslot: start at power-on: cannot update hook: {e}"),
        }
    }
}
