//! What each system's card says (the "Top Trumps" card on the top screen, in the system picker
//! and on the menu's page for the console): its name and maker, its stats, each with a bar to
//! compare it with the other systems', and a short history and a fun fact. The facts are in
//! `data/systems/<key>.toml`, one file per system, each citing where its figures come from
//! (the bio and the fun fact are DSlot's own words). A stat a source doesn't give is left out.

use crate::rom::Console;
use serde::Deserialize;
use std::sync::OnceLock;

/// A system's card.
#[derive(Deserialize, Debug)]
pub struct Card {
    pub name: String,
    pub maker: String,
    /// Where its figures come from.
    pub source: String,
    pub bio: String,
    pub fact: String,
    pub stats: Vec<Stat>,
}

/// A stat: what it is, as shown, and the number its bar is drawn from (none where it can't be
/// compared, a price in another currency, say).
#[derive(Deserialize, Debug)]
pub struct Stat {
    pub label: String,
    pub value: String,
    pub bar: Option<f64>,
    /// A bar that is a share of something (a battery's charge, a card's room), 0 to 1, drawn
    /// as it is rather than against the other systems.
    #[serde(skip)]
    pub share: Option<f64>,
}

/// The cards, by console: built into DSlot.
const FILES: [(Console, &str); 19] = [
    (Console::Ds, include_str!("../data/systems/ds.toml")),
    (Console::Gba, include_str!("../data/systems/gba.toml")),
    (Console::Gb, include_str!("../data/systems/gb.toml")),
    (Console::Gbc, include_str!("../data/systems/gbc.toml")),
    (Console::Snes, include_str!("../data/systems/snes.toml")),
    (Console::Nes, include_str!("../data/systems/nes.toml")),
    (Console::Fds, include_str!("../data/systems/fds.toml")),
    (
        Console::MegaDrive,
        include_str!("../data/systems/megadrive.toml"),
    ),
    (
        Console::MasterSystem,
        include_str!("../data/systems/mastersystem.toml"),
    ),
    (Console::Sg1000, include_str!("../data/systems/sg1000.toml")),
    (
        Console::GameGear,
        include_str!("../data/systems/gamegear.toml"),
    ),
    (
        Console::PcEngine,
        include_str!("../data/systems/pcengine.toml"),
    ),
    (
        Console::PcEngineCd,
        include_str!("../data/systems/pcenginecd.toml"),
    ),
    (
        Console::PlayStation,
        include_str!("../data/systems/playstation.toml"),
    ),
    (Console::Lynx, include_str!("../data/systems/lynx.toml")),
    (
        Console::NeoGeoPocket,
        include_str!("../data/systems/neogeopocket.toml"),
    ),
    (
        Console::WonderSwan,
        include_str!("../data/systems/wonderswan.toml"),
    ),
    (
        Console::GameWatch,
        include_str!("../data/systems/gamewatch.toml"),
    ),
    (Console::Ports, include_str!("../data/systems/ports.toml")),
];

fn cards() -> &'static [(Console, Card)] {
    static CARDS: OnceLock<Vec<(Console, Card)>> = OnceLock::new();
    CARDS.get_or_init(|| {
        FILES
            .iter()
            .filter_map(|&(console, text)| match toml::from_str(text) {
                Ok(card) => Some((console, card)),
                Err(e) => {
                    eprintln!("dslot: the {} card: {e}", console.label());
                    None
                }
            })
            .collect()
    })
}

/// A card's photo: a reduced copy (340 x 224) of a freely licensed photograph on Wikimedia
/// Commons, and its credit as the card shows it (data/systems/photos/CREDITS.md has each
/// one's source and licence).
pub struct Photo {
    pub png: &'static [u8],
    pub credit: &'static str,
}

macro_rules! photo {
    ($key:literal, $credit:expr) => {
        Photo {
            png: include_bytes!(concat!("../data/systems/photos/", $key, ".png")),
            credit: $credit,
        }
    };
}

const EVAN_AMOS: &str = "Photo: Evan Amos, public domain (Wikimedia Commons)";
const RETRO_EDITOR: &str = "Photo: RetroEditor, CC BY 4.0, cropped (Wikimedia Commons)";

const PHOTOS: [(Console, Photo); 18] = [
    (Console::Ds, photo!("ds", EVAN_AMOS)),
    (Console::Gba, photo!("gba", EVAN_AMOS)),
    (Console::Gb, photo!("gb", EVAN_AMOS)),
    (Console::Gbc, photo!("gbc", EVAN_AMOS)),
    (Console::Snes, photo!("snes", EVAN_AMOS)),
    (Console::Nes, photo!("nes", EVAN_AMOS)),
    (Console::Fds, photo!("fds", EVAN_AMOS)),
    (Console::MegaDrive, photo!("megadrive", EVAN_AMOS)),
    (Console::MasterSystem, photo!("mastersystem", EVAN_AMOS)),
    (Console::Sg1000, photo!("sg1000", EVAN_AMOS)),
    (Console::GameGear, photo!("gamegear", EVAN_AMOS)),
    (Console::PcEngine, photo!("pcengine", EVAN_AMOS)),
    (Console::PcEngineCd, photo!("pcenginecd", RETRO_EDITOR)),
    (Console::PlayStation, photo!("playstation", EVAN_AMOS)),
    (Console::Lynx, photo!("lynx", EVAN_AMOS)),
    (Console::NeoGeoPocket, photo!("neogeopocket", EVAN_AMOS)),
    (Console::WonderSwan, photo!("wonderswan", EVAN_AMOS)),
    (Console::GameWatch, photo!("gamewatch", EVAN_AMOS)),
];

/// `console`'s photo, if it has one.
pub fn photo(console: Console) -> Option<&'static Photo> {
    PHOTOS
        .iter()
        .find(|(c, _)| *c == console)
        .map(|(_, photo)| photo)
}

/// `console`'s card, if it has one (the apps' page has none).
pub fn card(console: Console) -> Option<&'static Card> {
    cards()
        .iter()
        .find(|(c, _)| *c == console)
        .map(|(_, card)| card)
}

/// How long a bar `value` of stat `label` is, of 1, against every system's: the year and the
/// price on a straight scale (the earliest year shortest), the rest, which run over a thousand
/// times or more (2 KB to 4 MB), on a logarithmic one. Never shorter than a sliver.
pub fn bar(label: &str, value: f64) -> f32 {
    let values: Vec<f64> = cards()
        .iter()
        .flat_map(|(_, card)| card.stats.iter())
        .filter(|s| s.label == label)
        .filter_map(|s| s.bar)
        .collect();
    let (lo, hi) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let t = match label {
        "Released" => (value - lo) / (hi - lo).max(1.0),
        "Launch price" => value / hi.max(1.0),
        _ => (value.ln() - lo.ln()) / (hi.ln() - lo.ln()).max(1e-9),
    };
    (0.12 + 0.88 * t.clamp(0.0, 1.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_card_cites_its_figures() {
        for (console, _) in FILES {
            let card = card(console).unwrap_or_else(|| panic!("{console:?}'s card doesn't read"));
            assert!(card.source.starts_with("https://"), "{console:?}");
            assert!(!card.bio.is_empty() && !card.fact.is_empty());
            // Ports aren't a console: no stats to give.
            if console != Console::Ports {
                assert!(card.stats.len() >= 5, "{console:?}");
            }
        }
        assert!(card(Console::Apps).is_none());
    }

    /// A system can't be added without its card: every page DSlot can show has one (the Apps
    /// page's is the device's own, device.rs).
    #[test]
    fn every_page_has_a_card() {
        for console in Console::PAGES {
            if console != Console::Apps {
                assert!(
                    FILES.iter().any(|&(c, _)| c == console),
                    "{console:?} has no card in data/systems/"
                );
            }
        }
    }

    #[test]
    fn every_card_has_its_photo() {
        // Ports aren't a console: a neutral card, with no photo.
        for (console, _) in FILES.into_iter().filter(|&(c, _)| c != Console::Ports) {
            let photo = photo(console).unwrap_or_else(|| panic!("{console:?} has no photo"));
            let img = crate::gfx::decode_png(photo.png).expect("a PNG");
            assert!(photo.credit.starts_with("Photo: "));
            assert_eq!((img.w, img.h), (340, 224));
        }
    }

    #[test]
    fn bars_compare_systems() {
        // The DS sold the most, the SG-1000 (in Japan) the least.
        assert_eq!(bar("Sold", 154.02), 1.0);
        assert!((bar("Sold", 0.4) - 0.12).abs() < 1e-6);
        assert!(bar("Sold", 61.91) > bar("Sold", 30.75));
        // The year: the Game & Watch's first, the DS's last.
        assert!(bar("Released", 1980.0) < bar("Released", 1990.0));
        assert_eq!(bar("Released", 2004.0), 1.0);
    }
}
