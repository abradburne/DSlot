//! The cards' game folders, made when they are missing: a folder for every system an installed
//! emulator plays (nibkit's `Emulators::systems`, the one list of them), so a new card or a
//! newly installed emulator shows where its games go. Each folder DSlot makes gets a short
//! `_README.txt`: what goes in it, the BIOS it needs, where its saves go.
//!
//! Only on a card that is already laid out for games: one with a `Roms` folder, or nibOS's own
//! data card (the one with its `Saves`, `/mnt/roms`). A card set up for slot CFW (`Games/`) gets slot's systems
//! (GBA, GB, GBC) there too. Nothing is ever renamed, moved or deleted, a file DSlot didn't make
//! is never written over, and a card with neither folder is left alone.

use crate::rom::Console;
use std::fs;
use std::path::{Path, PathBuf};

/// The README DSlot puts in a folder it makes.
pub const README: &str = "_README.txt";

/// The systems slot CFW plays, which its `Games/` gets.
const SLOT: [&str; 3] = ["GBA", "GB", "GBC"];

/// The cards DSlot reads: each `Roms` (or `Games`) folder's card.
pub fn cards(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut cards: Vec<PathBuf> = Vec::new();
    for card in roots.iter().filter_map(|r| r.parent()) {
        if !cards.iter().any(|c| c == card) {
            cards.push(card.to_path_buf());
        }
    }
    cards
}

/// [`make`] on a thread of its own (a card is slow to write), for the systems `emulators` play
/// and PortMaster's ports, on the handheld only: a desktop's folders are left alone.
pub fn make_soon(roots: &[PathBuf], emulators: &crate::emulator::Emulators) {
    if !crate::platform::on_device() {
        return;
    }
    let cards = cards(roots);
    let mut systems = emulators.systems();
    if !systems.iter().any(|s| s == "PORTS") {
        systems.push("PORTS".into());
    }
    let _ = std::thread::Builder::new()
        .name("folders".into())
        .spawn(move || {
            let made = make(&cards, &systems, nibos_data);
            if !made.is_empty() {
                eprintln!("dslot: made {} game folders", made.len());
            }
        });
}

/// Whether `card` is nibOS's own data card: the one its saves folder is on (`/mnt/roms`, the
/// image's NIBOS-DATA partition, or a development card's ROMS one).
pub fn nibos_data(card: &Path) -> bool {
    nibkit::paths::saves_root().parent() == Some(card)
}

/// Makes the missing system folders on `cards`, `systems` being the folder names (`NDS`,
/// `PORTS`...); `ours` says whether a card is nibOS's data partition. Returns the folders made.
pub fn make(cards: &[PathBuf], systems: &[String], ours: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut made = Vec::new();
    for card in cards {
        let (roms, games) = (card.join("Roms"), card.join("Games"));
        let mut layouts: Vec<(PathBuf, Vec<&String>)> = Vec::new();
        if roms.is_dir() || ours(card) {
            layouts.push((roms, systems.iter().collect()));
        }
        if games.is_dir() {
            let slot = systems.iter().filter(|s| SLOT.contains(&s.as_str()));
            layouts.push((games, slot.collect()));
        }
        for (root, systems) in layouts {
            for system in systems {
                let dir = root.join(system);
                if dir.exists() {
                    continue;
                }
                match fs::create_dir_all(&dir) {
                    Ok(()) => {
                        // A README of its own only: never over a file that is already there.
                        let _ = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(dir.join(README))
                            .and_then(|mut f| {
                                std::io::Write::write_all(&mut f, readme(system).as_bytes())
                            });
                        made.push(dir);
                    }
                    Err(e) => eprintln!("dslot: cannot make {}: {e}", dir.display()),
                }
            }
        }
    }
    made
}

/// The BIOS a system's emulator needs in `BIOS/` (nibcase's), and ones it plays better with.
fn bios(console: Console) -> (Option<&'static str>, &'static [&'static str]) {
    match console {
        Console::Lynx => (Some("lynxboot.img"), &[]),
        Console::PcEngineCd => (Some("syscard3.pce"), &[]),
        Console::Fds => (Some("disksys.rom"), &[]),
        Console::PlayStation => (None, &["scph5501.bin", "scph1001.bin", "scph7001.bin"]),
        _ => (None, &[]),
    }
}

/// What goes in a system's folder, in a few lines.
pub fn readme(system: &str) -> String {
    let Some(console) = Console::from_folder(system) else {
        return format!(
            "{system} games go in this folder.\n\n\
             Made by DSlot. You can delete this file.\n"
        );
    };
    let name = console.name();
    if console == Console::Ports {
        return format!(
            "{name}: PortMaster puts the ports it installs here, each a script (.sh) and a \
             folder of its own.\nOpen \"Get more ports\" on DSlot's Ports page to install some.\n\n\
             Made by DSlot. You can delete this file.\n"
        );
    }
    let files: Vec<String> = console
        .extensions()
        .iter()
        .map(|e| format!(".{e}"))
        .collect();
    let mut text = format!(
        "{name} games go in this folder.\n\nFiles: {}.\n",
        files.join(", ")
    );
    match bios(console) {
        (Some(need), _) => {
            text += &format!(
            "BIOS: {name} needs {need} in the BIOS folder at the card's root (your own copy).\n"
        )
        }
        (None, better) if !better.is_empty() => {
            text += &format!(
                "BIOS: none needed; with one of {} in the BIOS folder at the card's root, \
                 games play more accurately.\n",
                better.join(", ")
            )
        }
        _ => text += "BIOS: none needed.\n",
    }
    let saves = nibkit::paths::saves_root();
    text += &format!(
        "Saves: {}/{system}/<game>/, kept for you.\n\
         Box art (optional): Imgs/<game>.png beside the games.\n\n\
         Made by DSlot. You can delete this file.\n",
        saves.display()
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn systems() -> Vec<String> {
        ["GB", "GBA", "LYNX", "NDS", "PORTS"]
            .map(String::from)
            .to_vec()
    }

    /// A card with some folders gets the rest, each with a README; what was there is left as
    /// it was.
    #[test]
    fn fills_in_a_card() {
        let dir = TempDir::new("folders");
        let card = dir.path().join("card");
        fs::create_dir_all(card.join("Roms/NDS")).unwrap();
        fs::write(card.join("Roms/NDS/game.nds"), b"x").unwrap();
        let made = make(std::slice::from_ref(&card), &systems(), |_| false);
        assert_eq!(made.len(), 4);
        for s in ["GB", "GBA", "LYNX", "PORTS"] {
            assert!(card.join("Roms").join(s).join(README).is_file(), "{s}");
        }
        assert!(!card.join("Roms/NDS").join(README).exists());
        let lynx = fs::read_to_string(card.join("Roms/LYNX").join(README)).unwrap();
        assert!(
            lynx.contains("lynxboot.img") && lynx.contains(".lnx"),
            "{lynx}"
        );
        // Again: nothing more to make, and a README edited since is kept.
        fs::write(card.join("Roms/GB").join(README), "mine").unwrap();
        assert!(make(std::slice::from_ref(&card), &systems(), |_| false).is_empty());
        assert_eq!(
            fs::read_to_string(card.join("Roms/GB").join(README)).unwrap(),
            "mine"
        );
    }

    /// A card with neither Roms/ nor Games/ is untouched, unless it is nibOS's own; a slot
    /// card gets slot's systems under Games/ too.
    #[test]
    fn only_cards_laid_out_for_games() {
        let dir = TempDir::new("folders-cards");
        let (other, ours, slot) = (
            dir.path().join("other"),
            dir.path().join("ours"),
            dir.path().join("slot"),
        );
        for c in [&other, &ours] {
            fs::create_dir_all(c).unwrap();
        }
        fs::create_dir_all(slot.join("Games/GBA")).unwrap();
        let cards = [other.clone(), ours.clone(), slot.clone()];
        make(&cards, &systems(), |c| c == ours);
        assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
        assert!(ours.join("Roms/NDS").is_dir());
        // The slot card: Games/GB made, Games/GBA kept, nothing that isn't slot's; no Roms/.
        assert!(slot.join("Games/GB").is_dir());
        assert!(!slot.join("Games/NDS").exists() && !slot.join("Roms").exists());
        assert!(!slot.join("Games/GBA").join(README).exists());
    }

    #[test]
    fn cards_from_roots() {
        let roots = ["/a/Roms", "/b/Roms", "/a/Games", "/b/Games"].map(PathBuf::from);
        assert_eq!(cards(&roots), ["/a", "/b"].map(PathBuf::from));
    }
}
