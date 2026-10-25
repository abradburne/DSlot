//! What slot CFW's Cart Studio (studio.slot-cfw.fyi) puts on a card, read so that its labels and
//! shells work here too. The format is slot's, as its card's `Labels/README.txt` gives it:
//!
//! - a label per game in `Labels/<system>/<ROM name>.png` at the card's root (`GBA`, `GB`,
//!   `GBC`), any size, scaled to fill the sticker and cropped from the centre;
//! - `cart_shell.txt`, a line per game, `Metroid Fusion = auto 6a3fa0 solid`: the outline
//!   (auto, notched or rounded), the plastic's colour (hex RGB) and its finish (solid, clear or
//!   glitter).
//!
//! DSlot's own artwork (`Roms/<system>/Labels/`) comes first; Studio's is looked for after it,
//! on the game's own card and then on the other.

use crate::rom::Game;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

/// A cart's shape, where its system had more than one: the Game Boy's, with the notch cut from
/// its corner, or the Color-only cart's, rounded all round. `Auto` is the system's own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Outline {
    Auto,
    Notched,
    Rounded,
}

/// What the plastic is: opaque, see-through with the board showing, or flecked with glitter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Finish {
    Solid,
    Clear,
    Glitter,
}

/// One game's line of `cart_shell.txt`. With no colour (`auto`, or one that doesn't read), the
/// plastic keeps the system's own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Style {
    pub outline: Outline,
    pub colour: Option<[u8; 3]>,
    pub finish: Finish,
}

/// `cart_shell.txt`'s lines, by game. A line that doesn't read is left out, and so is a word
/// in it that doesn't: an unknown outline is `auto`, an unknown finish `solid`.
pub fn parse(text: &str) -> HashMap<String, Style> {
    let mut styles = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // A game's name may hold an `=`; the values never do.
        let Some((name, values)) = line.rsplit_once('=') else {
            continue;
        };
        let name = name.trim();
        let words: Vec<String> = values.split_whitespace().map(str::to_lowercase).collect();
        if name.is_empty() || words.is_empty() {
            continue;
        }
        let word = |i: usize| words.get(i).map_or("", String::as_str);
        let outline = match word(0) {
            "notched" => Outline::Notched,
            "rounded" => Outline::Rounded,
            _ => Outline::Auto,
        };
        let finish = match word(2) {
            "clear" => Finish::Clear,
            "glitter" => Finish::Glitter,
            _ => Finish::Solid,
        };
        styles.insert(
            name.to_string(),
            Style {
                outline,
                colour: hex(word(1)),
                finish,
            },
        );
    }
    styles
}

/// `6a3fa0` or `#6a3fa0`.
fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// The root of the card a game is on: `<card>/Roms/<system>/<rom>`, or slot's
/// `<card>/Games/<system>/<rom>`.
fn card_of(game: &Game) -> Option<PathBuf> {
    let roms = game.path.parent()?.parent()?;
    let layout = roms.file_name()?;
    (layout == "Roms" || layout == "Games").then(|| roms.parent().map(Path::to_path_buf))?
}

/// The cards to look on for `game`: its own, then the others DSlot reads.
pub fn cards(game: &Game) -> Vec<PathBuf> {
    static OTHERS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    let others = OTHERS.get_or_init(|| {
        nibkit::paths::rom_roots()
            .iter()
            .filter_map(|r| r.parent().map(Path::to_path_buf))
            .collect()
    });
    let mut cards: Vec<PathBuf> = card_of(game).into_iter().collect();
    for card in others {
        if !cards.contains(card) {
            cards.push(card.clone());
        }
    }
    cards
}

/// Where `game`'s label may be, in the order they're looked for: DSlot's own beside the ROM,
/// then Studio's on each card. With each, whether it is Studio's (cropped to fit, not stretched).
pub fn labels(game: &Game, cards: &[PathBuf]) -> Vec<(PathBuf, bool)> {
    let file = format!("{}.png", game.stem);
    let mut found = vec![(game.art("Labels"), false)];
    for card in cards {
        found.push((
            card.join("Labels").join(game.console.folder()).join(&file),
            true,
        ));
    }
    found
}

/// The `cart_shell.txt` files that may hold `game`'s line, in the order they're read: beside
/// the ROMs, then on each card in Studio's `Labels/<system>/` and `Labels/`, and at its root.
pub fn shell_files(game: &Game, cards: &[PathBuf]) -> Vec<PathBuf> {
    let name = "cart_shell.txt";
    let mut files: Vec<PathBuf> = game
        .path
        .parent()
        .map(|d| d.join(name))
        .into_iter()
        .collect();
    for card in cards {
        let labels = card.join("Labels");
        files.push(labels.join(game.console.folder()).join(name));
        files.push(labels.join(name));
        files.push(card.join(name));
    }
    files
}

/// `game`'s shell, from the first `cart_shell.txt` with a line for it.
pub fn style(game: &Game, cards: &[PathBuf]) -> Option<Style> {
    shell_files(game, cards)
        .iter()
        .find_map(|file| read(file)?.get(&game.stem).copied())
}

/// A `cart_shell.txt`, read once while it stays as it is.
fn read(file: &Path) -> Option<HashMap<String, Style>> {
    type Read = HashMap<PathBuf, (Option<SystemTime>, HashMap<String, Style>)>;
    static FILES: OnceLock<Mutex<Read>> = OnceLock::new();
    let meta = fs::metadata(file).ok()?;
    let stamp = meta.modified().ok();
    let mut files = FILES.get_or_init(Default::default).lock().unwrap();
    if let Some((s, styles)) = files.get(file) {
        if *s == stamp {
            return Some(styles.clone());
        }
    }
    let styles = parse(&fs::read_to_string(file).ok()?);
    files.insert(file.to_path_buf(), (stamp, styles.clone()));
    Some(styles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::Console;

    #[test]
    fn reads_lines() {
        let styles = parse(
            "# made with Cart Studio\n\
             Metroid Fusion = auto 6a3fa0 solid\n\
             Tetris = NOTCHED #8B8B8B Clear\n\
             Pokemon - Crystal Version (USA) = rounded 40e0d0 glitter\n\
             A = B (Japan) = rounded ff0000 solid\n\
             \n\
             no equals sign here\n\
             Odd = sideways nothex sparkly extra words\n\
             = auto 123456 solid\n",
        );
        assert_eq!(
            styles["Metroid Fusion"],
            Style {
                outline: Outline::Auto,
                colour: Some([0x6a, 0x3f, 0xa0]),
                finish: Finish::Solid
            }
        );
        assert_eq!(styles["Tetris"].outline, Outline::Notched);
        assert_eq!(styles["Tetris"].colour, Some([0x8b; 3]));
        assert_eq!(styles["Tetris"].finish, Finish::Clear);
        let crystal = styles["Pokemon - Crystal Version (USA)"];
        assert_eq!(
            (crystal.outline, crystal.finish),
            (Outline::Rounded, Finish::Glitter)
        );
        assert_eq!(styles["A = B (Japan)"].colour, Some([255, 0, 0]));
        // Unknown words fall back; a line with no name or no values is skipped.
        assert_eq!(
            styles["Odd"],
            Style {
                outline: Outline::Auto,
                colour: None,
                finish: Finish::Solid
            }
        );
        assert_eq!(styles.len(), 5);
    }

    #[test]
    fn lookup_order() {
        let dir = crate::testing::TempDir::new("studio");
        let (one, two) = (dir.path().join("one"), dir.path().join("two"));
        let gba = one.join("Roms/GBA");
        fs::create_dir_all(gba.join("Labels")).unwrap();
        let game = Game {
            console: Console::Gba,
            path: gba.join("Metroid Fusion.gba"),
            stem: "Metroid Fusion".into(),
            code: String::new(),
            title: vec!["Metroid Fusion".into()],
            icon: None,
            cover: None,
            gone: false,
        };
        assert_eq!(card_of(&game), Some(one.clone()));
        // A game in slot's own layout, Games/<system>/, is on its card just the same.
        let slot = Game {
            path: two.join("Games/GBA/Metroid Fusion.gba"),
            ..game.clone()
        };
        assert_eq!(card_of(&slot), Some(two.clone()));
        let cards = [one.clone(), two.clone()];
        let labels: Vec<PathBuf> = labels(&game, &cards).into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            labels,
            [
                gba.join("Labels/Metroid Fusion.png"),
                one.join("Labels/GBA/Metroid Fusion.png"),
                two.join("Labels/GBA/Metroid Fusion.png"),
            ]
        );

        // The other card's, then this card's Labels/, then beside the ROMs: the nearest wins.
        assert_eq!(style(&game, &cards), None);
        let write = |p: PathBuf, line: &str| {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, format!("Metroid Fusion = {line}\n")).unwrap();
        };
        write(two.join("Labels/cart_shell.txt"), "auto 000001 solid");
        assert_eq!(style(&game, &cards).unwrap().colour, Some([0, 0, 1]));
        write(one.join("cart_shell.txt"), "auto 000002 solid");
        assert_eq!(style(&game, &cards).unwrap().colour, Some([0, 0, 2]));
        write(one.join("Labels/cart_shell.txt"), "auto 000003 solid");
        assert_eq!(style(&game, &cards).unwrap().colour, Some([0, 0, 3]));
        write(one.join("Labels/GBA/cart_shell.txt"), "auto 000004 solid");
        assert_eq!(style(&game, &cards).unwrap().colour, Some([0, 0, 4]));
        write(gba.join("cart_shell.txt"), "auto 000005 clear");
        assert_eq!(style(&game, &cards).unwrap().finish, Finish::Clear);
    }
}
