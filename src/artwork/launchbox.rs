//! LaunchBox Games Database artwork index (`data/art/<console>.tsv.z`).
//!
//! Compact zlib-compressed TSV files extracted from LaunchBox's daily public `Metadata.zip` dump
//! (`tools/update-launchbox-art.py`). Loaded lazily per console only when artwork is requested,
//! so normal DSlot boot never touches them. Images are downloaded directly on demand from
//! `https://images.launchbox-app.com/<FileName>`.

use super::matching;
use crate::rom::Console;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const IMAGE_BASE: &str = "https://images.launchbox-app.com/";

/// Region-tagged image filenames for one game (`[("US", "abc.png"), ...]`).
struct Entry {
    carts: Vec<(String, String)>,
    boxes: Vec<(String, String)>,
}

/// One console's LaunchBox index, keyed by normalized title (primary + alternate names).
pub struct PlatformIndex {
    entries: Vec<Entry>,
    by_key: HashMap<String, Vec<usize>>,
}

impl PlatformIndex {
    /// Decompresses and parses a `.tsv.z` file.
    pub fn load(path: &std::path::Path) -> Option<PlatformIndex> {
        let compressed = std::fs::read(path).ok()?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).ok()?;
        let text = String::from_utf8(raw).ok()?;
        Some(Self::parse(&text))
    }

    pub fn parse(text: &str) -> PlatformIndex {
        let mut entries = Vec::new();
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut cols = line.split('\t');
            let Some(name) = cols.next() else { continue };
            let alts = cols.next().unwrap_or("");
            let carts = parse_pairs(cols.next().unwrap_or(""));
            let boxes = parse_pairs(cols.next().unwrap_or(""));
            if carts.is_empty() && boxes.is_empty() {
                continue;
            }
            let idx = entries.len();
            entries.push(Entry { carts, boxes });
            for title in std::iter::once(name).chain(alts.split('|').filter(|s| !s.is_empty())) {
                for key in matching::keys(title, false) {
                    let list = by_key.entry(key).or_default();
                    if list.last() != Some(&idx) {
                        list.push(idx);
                    }
                }
            }
        }
        PlatformIndex { entries, by_key }
    }

    /// Best LaunchBox image URL for `stem`, choosing `want_cart` (Cart - Front / Disc) or Box - Front,
    /// ranked by the ROM's region tag (`USA` / `World` / `Europe` / `Japan`).
    pub fn lookup(&self, stem: &str, want_cart: bool) -> Option<String> {
        let rom_regions = matching::regions(stem);
        let preferred = region_order(&rom_regions);
        for reduced in [false, true] {
            let mut seen = HashSet::new();
            let mut best: Option<(i32, &str)> = None;
            for key in matching::keys(stem, reduced) {
                let Some(ids) = self.by_key.get(&key) else {
                    continue;
                };
                for &id in ids {
                    if !seen.insert(id) {
                        continue;
                    }
                    let list = if want_cart {
                        &self.entries[id].carts
                    } else {
                        &self.entries[id].boxes
                    };
                    if let Some((rank, file)) = pick_best(list, &preferred) {
                        if best.is_none_or(|(b, _)| rank > b) {
                            best = Some((rank, file));
                        }
                    }
                }
            }
            if let Some((_, file)) = best {
                return Some(format!("{IMAGE_BASE}{file}"));
            }
        }
        None
    }
}

fn parse_pairs(col: &str) -> Vec<(String, String)> {
    col.split(';')
        .filter(|s| !s.is_empty())
        .map(|item| match item.split_once('=') {
            Some((r, f)) => (r.to_owned(), f.to_owned()),
            None => (String::new(), item.to_owned()),
        })
        .collect()
}

fn region_order(rom_regions: &HashSet<&str>) -> Vec<&'static str> {
    let mut order = Vec::new();
    let mut push = |code: &'static str| {
        if !order.contains(&code) {
            order.push(code);
        }
    };
    if rom_regions.contains("USA") {
        push("US");
    }
    if rom_regions.contains("Europe") {
        push("EU");
    }
    if rom_regions.contains("Japan") {
        push("JP");
    }
    if rom_regions.contains("World") {
        push("W");
    }
    for fallback in ["US", "W", "EU", "JP", "", "KR", "AS"] {
        push(fallback);
    }
    order
}

fn pick_best<'a>(pairs: &'a [(String, String)], order: &[&str]) -> Option<(i32, &'a str)> {
    let mut best: Option<(i32, &'a str)> = None;
    for (reg, file) in pairs {
        let pos = order
            .iter()
            .position(|&r| r == reg.as_str())
            .unwrap_or(order.len());
        let mut rank = 100 - pos as i32 * 10;
        if file.ends_with(".png") {
            rank += 2;
        }
        if best.is_none_or(|(b, _)| rank > b) {
            best = Some((rank, file.as_str()));
        }
    }
    best
}

/// The `.tsv.z` stem for `console` (matching `tools/update-launchbox-art.py`; GBA DS shares GBA).
pub fn console_key(console: Console) -> Option<&'static str> {
    Some(match console {
        Console::Ds => "ds",
        Console::Gba | Console::GbaDs => "gba",
        Console::Gb => "gb",
        Console::Gbc => "gbc",
        Console::Snes => "snes",
        Console::Nes => "nes",
        Console::Fds => "fds",
        Console::MegaDrive => "md",
        Console::MegaCd => "mcd",
        Console::Sega32x => "32x",
        Console::Pico => "pico",
        Console::MasterSystem => "sms",
        Console::Sg1000 => "sg",
        Console::GameGear => "gg",
        Console::PcEngine => "pce",
        Console::PcEngineCd => "pcecd",
        Console::PlayStation => "ps",
        Console::Psp => "psp",
        Console::Lynx => "lynx",
        Console::NeoGeoPocket => "ngp",
        Console::WonderSwan => "ws",
        Console::GameWatch => "gw",
        Console::Arcade => "arcade",
        Console::Ports | Console::Apps => return None,
    })
}

/// Finds `<key>.tsv.z` across the configured index directories.
pub fn find_file(dirs: &[PathBuf], key: &str) -> Option<PathBuf> {
    let name = format!("{key}.tsv.z");
    dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_region_and_alternate_names() {
        let tsv = "\
# 2026-11-24 launchbox
Super Mario World\tSuper Mario Bros. 4\tJP=jp-cart.png;US=us-cart.png\tUS=us-box.jpg
3 Ninjas Kick Back\t\tUS=ninjas.png\t
";
        let idx = PlatformIndex::parse(tsv);
        assert_eq!(
            idx.lookup("Super Mario World (USA)", true).as_deref(),
            Some("https://images.launchbox-app.com/us-cart.png")
        );
        assert_eq!(
            idx.lookup("Super Mario Bros. 4 (Japan)", true).as_deref(),
            Some("https://images.launchbox-app.com/jp-cart.png")
        );
        assert_eq!(
            idx.lookup("Super Mario World (USA)", false).as_deref(),
            Some("https://images.launchbox-app.com/us-box.jpg")
        );
        assert_eq!(idx.lookup("Super Mario World 2 (USA)", true), None);
    }

    #[test]
    fn every_console_index_loads_cleanly() {
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/data/art"));
        for console in Console::ALL {
            let Some(key) = console_key(console) else {
                continue;
            };
            let path = find_file(std::slice::from_ref(&dir), key)
                .unwrap_or_else(|| panic!("missing LaunchBox index for {console:?} ({key}.tsv.z)"));
            let idx = PlatformIndex::load(&path)
                .unwrap_or_else(|| panic!("failed to load LaunchBox index {}", path.display()));
            assert!(
                !idx.entries.is_empty(),
                "empty LaunchBox index for {console:?}"
            );
        }
    }
}
