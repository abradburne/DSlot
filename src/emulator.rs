//! The emulators DSlot can start games in, as their description files say. nibkit's `dslot`
//! module reads them (docs/emulators.md); DSlot names a description's systems by its consoles'
//! folder names (`Console::folder`).

use std::path::PathBuf;
use std::time::SystemTime;

pub use nibkit::dslot::{Action, Emulator, Emulators, Seed};

/// The parsed, available registry is deterministic (file order, then stable priority order).
/// Comparing it also notices required cores/programs appearing without a descriptor edit.
pub fn signature(emulators: &Emulators) -> String {
    format!("{emulators:?}")
}

/// Fast mtime fingerprint of the emulator descriptor directories (and standard package/core
/// directories when present) so the shelf does not re-read and re-parse every `.toml` file
/// every few seconds while idle.
pub fn dirs_stamp(dirs: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut out = Vec::new();
    let mtime = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    for dir in dirs {
        out.push((dir.clone(), mtime(dir)));
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                out.push((p.clone(), mtime(&p)));
            }
        }
    }
    for sys in ["/usr/bin", "/usr/lib/libretro", "/var/lib/pacman/local"] {
        let p = PathBuf::from(sys);
        if let Some(t) = mtime(&p) {
            out.push((p, Some(t)));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::Console;
    use std::path::Path;

    #[test]
    fn registry_signature_notices_new_integrations_and_options() {
        let description = |label: &str| {
            format!("name = \"{label}\"\n[systems]\nSFC = [\"/usr/bin/nibcase\", \"{{rom}}\"]\n")
        };
        let empty = Emulators::default();
        let first =
            Emulators::assume_installed([(String::from("nibcase-snes9x"), description("Snes9x"))]);
        let again =
            Emulators::assume_installed([(String::from("nibcase-snes9x"), description("Snes9x"))]);
        let changed = Emulators::assume_installed([(
            String::from("nibcase-snes9x"),
            description("New core"),
        )]);
        assert_ne!(signature(&empty), signature(&first));
        assert_eq!(signature(&first), signature(&again));
        assert_ne!(signature(&first), signature(&changed));
    }

    /// The descriptions shipped with the stock build, read as on the handheld, which has
    /// everything they name.
    #[test]
    fn shipped_descriptions() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("packaging/stock-card/Roms/APPS/dslot/emulators");
        let texts: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .map(|p| {
                (
                    p.file_stem().unwrap().to_string_lossy().into_owned(),
                    std::fs::read_to_string(&p).unwrap(),
                )
            })
            .collect();
        for (id, text) in &texts {
            assert!(nibkit::dslot::check(id, text).is_ok(), "{id}");
        }
        let emulators = Emulators::assume_installed(texts);
        let default = |c: Console| {
            emulators
                .for_system(c.folder())
                .next()
                .map(|e| e.id.clone())
        };
        // As DSlot 0.2 did; GBA as the stock menu does by default; GB and GBC in inkdot where
        // it is installed (Gambatte where not).
        assert_eq!(default(Console::Ds).as_deref(), Some("nnddss"));
        assert_eq!(default(Console::Gba).as_deref(), Some("mgba"));
        assert_eq!(default(Console::Gbc).as_deref(), Some("inkdot"));
        assert_eq!(default(Console::Snes).as_deref(), Some("snes9x"));
        assert_eq!(default(Console::Nes).as_deref(), Some("nestopia"));
        assert_eq!(
            default(Console::MegaDrive).as_deref(),
            Some("genesis_plus_gx")
        );
        assert_eq!(default(Console::GameGear).as_deref(), Some("gearsystem"));
        assert_eq!(default(Console::Lynx).as_deref(), Some("handy"));
        assert_eq!(
            emulators.for_system("GB").nth(1).map(|e| e.id.as_str()),
            Some("gambatte")
        );
        assert!(emulators.get("nnddss").unwrap().quit_on_output.is_some());
    }
}
