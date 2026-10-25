//! The consoles DSlot has pages for, and what each one's ROMs say about themselves. DS: game
//! code, banner title and the 32x32 icon. GBA, GB and GBC: game code (where there is one) and
//! maker from the cartridge header, also inside a .zip; the name is the file's. SNES uses the
//! same filename rule, with its LoROM/HiROM/ExHiROM headers read by `snes`.

mod ports;
mod snes;

pub use snes::north_american as snes_north_american;

use crate::gfx::Img;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Which shelf page a game is on, and which emulator the launcher gives it to. Each console has
/// its ROM folder (`Roms/<folder>`), file extension (or a .zip holding such a file), header
/// reader, cart shell and slot (`art.rs`) and box art (`Imgs/` beside its ROMs). A console gets
/// a page when an emulator for it is installed; where DSlot draws no cart for it (a Game & Watch
/// unit, a disk or a disc, or a cart it has no shell for yet), the page shows boxes alone.
///
/// `Apps` is not a console: it is the page of apps in `Roms/APPS` (nibkit::dslot::apps), there
/// when there is at least one, shown as a cover grid of their icons.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Console {
    Ds,
    Gba,
    Gb,
    Gbc,
    Snes,
    Nes,
    /// The Famicom Disk System's disks: no carts.
    Fds,
    MegaDrive,
    MasterSystem,
    Sg1000,
    GameGear,
    PcEngine,
    /// The PC Engine CD's discs: no carts.
    PcEngineCd,
    /// The PlayStation's discs: no carts.
    PlayStation,
    Lynx,
    NeoGeoPocket,
    WonderSwan,
    /// Nintendo's Game & Watch units: no carts, the unit is the game.
    GameWatch,
    /// PortMaster's ports, from `Roms/PORTS`: each a script started by PortMaster's runner,
    /// its name and picture from its folder (rom/ports.rs).
    Ports,
    Apps,
}

impl Console {
    /// The consoles, as against the apps, in tab order.
    pub const ALL: [Console; 19] = [
        Console::Ds,
        Console::Gba,
        Console::Gb,
        Console::Gbc,
        Console::Snes,
        Console::Nes,
        Console::Fds,
        Console::MegaDrive,
        Console::MasterSystem,
        Console::Sg1000,
        Console::GameGear,
        Console::PcEngine,
        Console::PcEngineCd,
        Console::PlayStation,
        Console::Lynx,
        Console::NeoGeoPocket,
        Console::WonderSwan,
        Console::GameWatch,
        Console::Ports,
    ];
    /// Every page there can be, in tab order: the consoles, then the apps.
    pub const PAGES: [Console; 20] = [
        Console::Ds,
        Console::Gba,
        Console::Gb,
        Console::Gbc,
        Console::Snes,
        Console::Nes,
        Console::Fds,
        Console::MegaDrive,
        Console::MasterSystem,
        Console::Sg1000,
        Console::GameGear,
        Console::PcEngine,
        Console::PcEngineCd,
        Console::PlayStation,
        Console::Lynx,
        Console::NeoGeoPocket,
        Console::WonderSwan,
        Console::GameWatch,
        Console::Ports,
        Console::Apps,
    ];
    /// How many there are, for tables by `Console as usize`.
    pub const COUNT: usize = Self::PAGES.len();

    /// The folder under `Roms/`, as the stock OS names them; also `Saves/<folder>/`.
    pub fn folder(self) -> &'static str {
        match self {
            Console::Ds => "NDS",
            Console::Gba => "GBA",
            Console::Gb => "GB",
            Console::Gbc => "GBC",
            Console::Snes => "SFC",
            Console::Nes => "FC",
            Console::Fds => "FDS",
            Console::MegaDrive => "MD",
            Console::MasterSystem => "SMS",
            Console::Sg1000 => "SG1000",
            Console::GameGear => "GG",
            Console::PcEngine => "PCE",
            Console::PcEngineCd => "PCECD",
            Console::PlayStation => "PS",
            Console::Lynx => "LYNX",
            Console::NeoGeoPocket => "NGP",
            Console::WonderSwan => "WS",
            Console::GameWatch => "GW",
            Console::Ports => "PORTS",
            Console::Apps => "APPS",
        }
    }

    /// Its full name, as the system picker shows it.
    pub fn name(self) -> &'static str {
        match self {
            Console::Ds => "Nintendo DS",
            Console::Gba => "Game Boy Advance",
            Console::Gb => "Game Boy",
            Console::Gbc => "Game Boy Color",
            Console::Snes => "Super Nintendo",
            Console::Nes => "NES / Famicom",
            Console::Fds => "Famicom Disk System",
            Console::MegaDrive => "Mega Drive / Genesis",
            Console::MasterSystem => "Master System",
            Console::Sg1000 => "SG-1000",
            Console::GameGear => "Game Gear",
            Console::PcEngine => "PC Engine",
            Console::PcEngineCd => "PC Engine CD",
            Console::PlayStation => "PlayStation",
            Console::Lynx => "Atari Lynx",
            Console::NeoGeoPocket => "Neo Geo Pocket",
            Console::WonderSwan => "WonderSwan",
            Console::GameWatch => "Game & Watch",
            Console::Ports => "Ports",
            Console::Apps => "Apps",
        }
    }

    /// On the page's tab.
    pub fn label(self) -> &'static str {
        match self {
            Console::Ds => "DS",
            Console::Gba => "GBA",
            Console::Gb => "GB",
            Console::Gbc => "GBC",
            Console::Snes => "SNES",
            Console::Nes => "NES",
            Console::Fds => "FDS",
            Console::MegaDrive => "MD",
            Console::MasterSystem => "SMS",
            Console::Sg1000 => "SG",
            Console::GameGear => "GG",
            Console::PcEngine => "PCE",
            Console::PcEngineCd => "PCE CD",
            Console::PlayStation => "PS",
            Console::Lynx => "Lynx",
            Console::NeoGeoPocket => "NGP",
            Console::WonderSwan => "WS",
            Console::GameWatch => "G&W",
            Console::Ports => "Ports",
            Console::Apps => "Apps",
        }
    }

    /// In `settings.ini`.
    pub fn key(self) -> &'static str {
        match self {
            Console::Ds => "ds",
            Console::Gba => "gba",
            Console::Gb => "gb",
            Console::Gbc => "gbc",
            Console::Snes => "snes",
            Console::Nes => "nes",
            Console::Fds => "fds",
            Console::MegaDrive => "megadrive",
            Console::MasterSystem => "mastersystem",
            Console::Sg1000 => "sg1000",
            Console::GameGear => "gamegear",
            Console::PcEngine => "pcengine",
            Console::PcEngineCd => "pcenginecd",
            Console::PlayStation => "playstation",
            Console::Lynx => "lynx",
            Console::NeoGeoPocket => "neogeopocket",
            Console::WonderSwan => "wonderswan",
            Console::GameWatch => "gamewatch",
            Console::Ports => "ports",
            Console::Apps => "apps",
        }
    }

    pub fn from_key(key: &str) -> Option<Console> {
        Console::PAGES.into_iter().find(|s| s.key() == key)
    }

    /// By its folder name, as emulator descriptions and launch.conf name consoles.
    pub fn from_folder(folder: &str) -> Option<Console> {
        Console::PAGES.into_iter().find(|s| s.folder() == folder)
    }

    /// The ROM file's extension (a CD game's: its cue sheet).
    pub fn ext(self) -> &'static str {
        match self {
            Console::Ds => "nds",
            Console::Gba => "gba",
            Console::Gb => "gb",
            Console::Gbc => "gbc",
            Console::Snes => "sfc",
            Console::Nes => "nes",
            Console::Fds => "fds",
            Console::MegaDrive => "md",
            Console::MasterSystem => "sms",
            Console::Sg1000 => "sg",
            Console::GameGear => "gg",
            Console::PcEngine => "pce",
            Console::PcEngineCd => "cue",
            Console::PlayStation => "pbp",
            Console::Lynx => "lnx",
            Console::NeoGeoPocket => "ngp",
            Console::WonderSwan => "ws",
            Console::GameWatch => "mgw",
            Console::Ports => "sh",
            // Apps are folders, not ROM files: never scanned for.
            Console::Apps => "",
        }
    }

    /// Whether a file with this extension (lower case) in the console's folder is one of its
    /// games: its ROM, or a zip. A Game Boy game may be either kind, in either folder: many
    /// Color games run on the first Game Boy too. A CD game is its cue sheet (the tracks it
    /// lists are not games of their own), a CHD, or a playlist of discs (.m3u, whose discs are
    /// not games of their own either), never zipped.
    fn takes(self, ext: &str) -> bool {
        self.extensions().contains(&ext)
    }

    /// The extensions its games have (lower case), as [`Console::takes`] takes them.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Console::Ds => &["nds", "zip"],
            Console::Gba => &["gba", "zip"],
            Console::Gb | Console::Gbc => &["gb", "gbc", "zip"],
            Console::Snes => &["sfc", "smc", "zip"],
            Console::Nes => &["nes", "unf", "unif", "zip"],
            Console::Fds => &["fds", "zip"],
            Console::MegaDrive => &["md", "gen", "bin", "smd", "zip"],
            Console::MasterSystem => &["sms", "zip"],
            Console::Sg1000 => &["sg", "zip"],
            Console::GameGear => &["gg", "zip"],
            Console::PcEngine => &["pce", "zip"],
            Console::PcEngineCd => &["cue", "chd", "m3u"],
            Console::PlayStation => &["m3u", "chd", "cue", "pbp"],
            Console::Lynx => &["lnx", "zip"],
            Console::NeoGeoPocket => &["ngp", "ngc", "zip"],
            Console::WonderSwan => &["ws", "wsc", "zip"],
            Console::Ports => &["sh"],
            // A unit as a MAME set, zipped or 7-Zipped, as well as converted (.mgw).
            Console::GameWatch => &["mgw", "zip", "7z"],
            Console::Apps => &[],
        }
    }

    /// Where what its ROMs said is kept between starts, in `cache/`.
    pub fn known(self) -> &'static str {
        match self {
            Console::Ds => "games.bin",
            Console::Gba => "gba.bin",
            Console::Gb => "gb.bin",
            Console::Gbc => "gbc.bin",
            Console::Snes => "snes.bin",
            Console::Nes => "nes.bin",
            Console::Fds => "fds.bin",
            Console::MegaDrive => "megadrive.bin",
            Console::MasterSystem => "mastersystem.bin",
            Console::Sg1000 => "sg1000.bin",
            Console::GameGear => "gamegear.bin",
            Console::PcEngine => "pcengine.bin",
            Console::PcEngineCd => "pcenginecd.bin",
            Console::PlayStation => "playstation.bin",
            Console::Lynx => "lynx.bin",
            Console::NeoGeoPocket => "neogeopocket.bin",
            Console::WonderSwan => "wonderswan.bin",
            Console::GameWatch => "gamewatch.bin",
            Console::Ports => "ports.bin",
            Console::Apps => "apps.bin",
        }
    }

    /// The prefix of the serial printed on a cart.
    fn serial(self) -> &'static str {
        match self {
            Console::Ds => "NTR",
            Console::Gba => "AGB",
            Console::Gb => "DMG",
            Console::Gbc => "CGB",
            Console::Snes => "SNS",
            Console::Nes => "NES",
            Console::Fds => "",
            Console::MegaDrive => "",
            Console::MasterSystem => "",
            Console::Sg1000 => "",
            Console::GameGear => "",
            Console::PcEngine => "",
            Console::PcEngineCd => "",
            Console::PlayStation => "",
            Console::Lynx => "",
            Console::NeoGeoPocket => "",
            Console::WonderSwan => "",
            Console::GameWatch => "",
            Console::Ports => "",
            Console::Apps => "",
        }
    }
}

#[derive(Clone)]
pub struct Game {
    pub console: Console,
    pub path: PathBuf,
    pub stem: String,
    /// Four-character game code from the header, e.g. `ALTE`. Empty when unreadable.
    pub code: String,
    /// Banner title split into lines; the first is the game's name.
    pub title: Vec<String>,
    pub icon: Option<Img>,
    /// Box art kept somewhere of its own rather than in `Imgs/` beside it: an app's icon.
    pub cover: Option<PathBuf>,
    /// Its file was there when DSlot started (or came since) and is not any more. A game that
    /// goes keeps its place in the list, so that every index into it stays good while DSlot
    /// runs; the shelf doesn't show it.
    pub gone: bool,
}

impl Game {
    /// Whether it is started as an app (nibkit's `app.toml`) rather than by an emulator: what
    /// is on the Apps page, and on the Ports page the one that isn't a port's script,
    /// PortMaster's own tile ([`ports::get_more`]).
    pub fn is_app(&self) -> bool {
        self.console == Console::Apps
            || self.console == Console::Ports
                && !self
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("sh"))
    }

    pub fn name(&self) -> &str {
        &self.title[0]
    }

    /// `NTR-ALTE-USA` or `AGB-AWRE-USA`, as printed on a real card.
    pub fn serial(&self) -> String {
        if self.code.is_empty() {
            return String::new();
        }
        let region = match self.code.as_bytes()[3] {
            b'E' => "-USA",
            b'J' => "-JPN",
            b'P' => "-EUR",
            b'K' => "-KOR",
            b'U' => "-AUS",
            _ => "",
        };
        let prefix = if self.console == Console::Snes && self.code.ends_with('J') {
            "SHVC"
        } else {
            self.console.serial()
        };
        format!("{prefix}-{}{region}", self.code)
    }

    /// What the settings call the game: its console's folder and its ROM's name, `NDS/Golden
    /// Sun - Dark Dawn (USA)`, which tells it from a game of another console of the same name.
    pub fn key(&self) -> String {
        nibkit::settings::Scope::game_key(self.console.folder(), &self.stem)
    }

    /// `<rom dir>/<folder>/<rom name>.png`; an app's own icon for its box (`Imgs`).
    pub fn art(&self, folder: &str) -> PathBuf {
        if let (Some(cover), "Imgs") = (&self.cover, folder) {
            return cover.clone();
        }
        let dir = self.path.parent().unwrap_or(Path::new("."));
        dir.join(folder).join(format!("{}.png", self.stem))
    }
}

/// `known` remembers what each ROM said the last time, so that a start only lists the folders
/// and opens the ROMs it has not met. A ROM is known by its path alone: asking the card for
/// each file's size and date costs as much as reading it. Deleting the file has every ROM
/// read again.
pub fn scan(console: Console, dirs: &[PathBuf], known: &Path) -> Vec<Game> {
    let old = read_known(known).unwrap_or_default();
    if console == Console::Ds {
        crate::mark("known games read");
    }
    let mut fresh = false;
    let mut games = Vec::new();
    let mut roms = 0;
    let roots: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| d.parent().map(Path::to_path_buf))
        .collect();
    for dir in dirs {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        // A port's name and picture are its folder's, read afresh each time (rom/ports.rs).
        let ports = if console == Console::Ports {
            ports::index(dir)
        } else {
            HashMap::new()
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let Some(stem) = rom_name(&path) else {
                continue;
            };
            if stem.starts_with('.') || !console.takes(&ext) {
                continue;
            }
            // A card set up for slot CFW keeps its games in Games/: one that is in Roms/ as
            // well is listed once, as the one in Roms/.
            if nibkit::paths::slot_copy(&path, &roots) {
                continue;
            }
            // A zipped DS game is listed by its file name.
            if console == Console::Ds && ext == "zip" {
                games.push(Game {
                    console,
                    path,
                    title: vec![stem.clone()],
                    stem,
                    code: String::new(),
                    icon: None,
                    cover: None,
                    gone: false,
                });
                continue;
            }
            if console == Console::Ports {
                let port = entry
                    .file_name()
                    .to_str()
                    .and_then(|f| ports.get(f))
                    .cloned()
                    .unwrap_or_default();
                // On both cards: the DS card's (the later folder) only.
                games.retain(|g: &Game| g.stem != stem);
                games.push(Game {
                    console,
                    title: vec![port.title.unwrap_or_else(|| stem.clone())],
                    path,
                    stem,
                    code: String::new(),
                    icon: None,
                    cover: port.picture,
                    gone: false,
                });
                continue;
            }
            roms += 1;
            let (code, title, icon) = match old.get(&path) {
                Some(k) => (k.code.clone(), k.title.clone(), k.icon.clone()),
                None => {
                    fresh = true;
                    read_header(console, &path, &stem)
                }
            };
            let title = if title.is_empty() {
                vec![stem.clone()]
            } else {
                title
            };
            games.push(Game {
                console,
                path,
                stem,
                code,
                title,
                icon,
                cover: None,
                gone: false,
            });
        }
    }
    // A playlist (.m3u) is a game of several discs: the discs it lists are not games of their
    // own.
    let listed = listed_discs(&games);
    if !listed.is_empty() {
        let before = games.len();
        games.retain(|g| !listed.contains(&g.path));
        roms -= before - games.len();
    }
    // Rewritten when a ROM was read, and when one has gone.
    if fresh || roms != old.len() {
        if let Err(e) = write_known(known, &games) {
            eprintln!("dslot: cannot write {}: {e}", known.display());
        }
    }
    games.sort_by_key(|g| g.name().to_lowercase());
    // PortMaster itself heads the Ports page, to get more: there with no ports yet too.
    if console == Console::Ports {
        games.extend(ports::get_more());
    }
    games
}

/// The discs the playlists among `games` list (an .m3u lists a game's discs, one file to a
/// line, beside it; `#` lines are comments).
fn listed_discs(games: &[Game]) -> std::collections::HashSet<PathBuf> {
    games
        .iter()
        .filter(|g| {
            g.path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("m3u"))
        })
        .filter_map(|g| {
            Some((
                g.path.parent()?.to_owned(),
                fs::read_to_string(&g.path).ok()?,
            ))
        })
        .flat_map(|(dir, list)| {
            list.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| dir.join(l))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Names the games an emulator lists (its `[games]` file: a MAME set by its name, say) as it
/// lists them: the title, and under it the model, year and series it gives.
pub fn name_from_lists(games: &mut [Game], emulators: &crate::emulator::Emulators) {
    for g in games.iter_mut() {
        if let Some(u) = emulators.unit(g.console.folder(), &g.stem) {
            let about: Vec<&str> = [u.model.as_str(), u.year.as_str(), u.series.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            g.title = std::iter::once(u.title.clone())
                .chain((!about.is_empty()).then(|| about.join("  \u{b7}  ")))
                .collect();
        }
    }
}

/// The apps found in `Roms/APPS`, as games on the Apps page: an app's folder (or the stock
/// menu's script) is its path, its folder's name its stem, its description the line under its
/// name, its icon its box.
pub fn apps(found: &[nibkit::dslot::Found]) -> Vec<Game> {
    let game = |f: &nibkit::dslot::Found| {
        let path = app_path(f);
        let title = std::iter::once(f.app.shown_name().to_owned())
            .chain(f.app.shown_description().map(str::to_owned))
            .collect();
        Game {
            console: Console::Apps,
            path,
            stem: f.id.clone(),
            code: String::new(),
            title,
            icon: None,
            cover: f.icon.clone(),
            gone: false,
        }
    };
    found.iter().map(game).collect()
}

/// What an app is known by on the shelf (its play time, favourite and rating): its folder, or
/// the stock menu's script it is.
pub fn app_path(f: &nibkit::dslot::Found) -> PathBuf {
    match f.app.command.as_slice() {
        [bash, script] if bash == "/bin/bash" => PathBuf::from(script),
        _ => f.folder.clone(),
    }
}

/// What a ROM says about itself: its game code, its title lines and its icon.
fn read_header(console: Console, path: &Path, stem: &str) -> (String, Vec<String>, Option<Img>) {
    match console {
        Console::Ds => parse(path).unwrap_or_default(),
        Console::Gba => parse_gba(path, stem),
        Console::Gb | Console::Gbc => parse_gb(path, stem),
        Console::Snes => snes::parse(path, stem),
        // NES cartridge dumps have no universal title/publisher/serial fields. Keep the
        // filename, including for NES 2.0 and UNIF, instead of inventing cartridge metadata.
        // A Game & Watch file (.mgw, a converted unit) has no header either, and the newer
        // systems' names come from their files too.
        Console::Nes
        | Console::MegaDrive
        | Console::GameGear
        | Console::Lynx
        | Console::GameWatch
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::PcEngineCd
        | Console::PlayStation
        | Console::NeoGeoPocket
        | Console::WonderSwan => (String::new(), vec![tidy(stem)], None),
        // An app says nothing about itself in a header: its app.toml has been read, and a
        // port's folder (`scan`).
        Console::Apps | Console::Ports => (String::new(), vec![stem.to_owned()], None),
    }
}

/// One ROM file as a game, outside a scan (`dslot --launch`). Its console is its folder's
/// (`Roms/GBA/...`), or else its extension's; a .zip outside a console's folder is a DS game.
pub fn read_one(path: &Path) -> Option<Game> {
    let folder = path.parent()?.file_name()?.to_str()?;
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    // A zip by the extension under it: `Tetris DX (World).gbc.zip`.
    let inner = (ext == "zip")
        .then(|| Path::new(path.file_stem()?).extension()?.to_str())
        .flatten()
        .map(str::to_ascii_lowercase);
    let rom_ext = inner.as_deref().unwrap_or(&ext);
    let by_ext = Console::ALL.into_iter().find(|c| {
        c.ext() == rom_ext
            || (*c == Console::Snes && rom_ext == "smc")
            || (*c == Console::Nes && matches!(rom_ext, "unf" | "unif"))
            || (*c == Console::MegaDrive && matches!(rom_ext, "gen" | "bin" | "smd"))
            || (*c == Console::NeoGeoPocket && rom_ext == "ngc")
            || (*c == Console::WonderSwan && rom_ext == "wsc")
            || (*c == Console::PcEngineCd && rom_ext == "chd")
    });
    let console = Console::from_folder(folder)
        .or(by_ext)
        .unwrap_or(Console::Ds);
    let stem = rom_name(path)?;
    let zipped_ds = console == Console::Ds && ext == "zip";
    let (code, title, icon) = if zipped_ds {
        Default::default()
    } else {
        read_header(console, path, &stem)
    };
    let title = if title.is_empty() {
        vec![stem.clone()]
    } else {
        title
    };
    Some(Game {
        console,
        path: path.to_owned(),
        stem,
        code,
        title,
        icon,
        cover: None,
        gone: false,
    })
}

/// A ROM's name: its file's without the extension, and without the ROM's own extension under a
/// zip's (`Tetris DX (World).gbc.zip` is `Tetris DX (World)`). Its artwork and saves go by it.
pub fn rom_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let zipped = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let inner = Path::new(stem).extension().and_then(|e| e.to_str());
    let rom_ext = |e: &str| {
        Console::ALL
            .into_iter()
            .any(|c| e != "zip" && c.takes(&e.to_ascii_lowercase()))
    };
    Some(match inner {
        Some(e) if zipped && rom_ext(e) => stem[..stem.len() - e.len() - 1].to_owned(),
        _ => stem.to_owned(),
    })
}

/// One ROM as the last scan found it.
struct Known {
    code: String,
    title: Vec<String>,
    icon: Option<Img>,
}

const KNOWN_MAGIC: &[u8] = b"dslot games 2\n";

fn write_known(file: &Path, games: &[Game]) -> std::io::Result<()> {
    let text = |out: &mut Vec<u8>, s: &str| {
        out.extend((s.len() as u32).to_le_bytes());
        out.extend(s.as_bytes());
    };
    let mut out = KNOWN_MAGIC.to_vec();
    // Everything but a zipped DS game, which is listed by its name alone.
    let read = |g: &&Game| {
        g.console != Console::Ds
            || g.path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nds"))
    };
    for g in games.iter().filter(read) {
        text(&mut out, &g.path.to_string_lossy());
        text(&mut out, &g.code);
        text(&mut out, &g.title.join("\n"));
        match &g.icon {
            Some(icon) if icon.px.len() == 32 * 32 * 4 => {
                out.push(1);
                out.extend(&icon.px);
            }
            _ => out.push(0),
        }
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(file, out)
}

/// `None` for a missing, older or damaged file: everything is then read from the ROMs again.
fn read_known(file: &Path) -> Option<HashMap<PathBuf, Known>> {
    let bytes = fs::read(file).ok()?;
    let mut rest = bytes.strip_prefix(KNOWN_MAGIC)?;
    fn take<'a>(rest: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if rest.len() < n {
            return None;
        }
        let (head, tail) = rest.split_at(n);
        *rest = tail;
        Some(head)
    }
    fn text(rest: &mut &[u8]) -> Option<String> {
        let n = u32::from_le_bytes(take(rest, 4)?.try_into().ok()?) as usize;
        String::from_utf8(take(rest, n)?.to_vec()).ok()
    }
    let mut known = HashMap::new();
    while !rest.is_empty() {
        let path = PathBuf::from(text(&mut rest)?);
        let code = text(&mut rest)?;
        let title = text(&mut rest)?.lines().map(str::to_owned).collect();
        let icon = match take(&mut rest, 1)?[0] {
            0 => None,
            _ => Some(Img {
                w: 32,
                h: 32,
                px: take(&mut rest, 32 * 32 * 4)?.to_vec(),
            }),
        };
        known.insert(path, Known { code, title, icon });
    }
    Some(known)
}

const BANNER_LEN: usize = 0x840;
const ICON_BITMAP: usize = 0x20;
const ICON_PALETTE: usize = 0x220;
const TITLE_ENGLISH: usize = 0x340;

fn parse(path: &Path) -> Option<(String, Vec<String>, Option<Img>)> {
    let mut f = File::open(path).ok()?;
    let mut head = [0u8; 0x70];
    f.read_exact(&mut head).ok()?;
    let code: String = head[0x0C..0x10].iter().map(|&b| b as char).collect();
    let code = if code.chars().all(|c| c.is_ascii_alphanumeric()) {
        code
    } else {
        String::new()
    };

    let offset = u32::from_le_bytes(head[0x68..0x6C].try_into().ok()?);
    let mut banner = [0u8; BANNER_LEN];
    if offset == 0
        || f.seek(SeekFrom::Start(offset as u64)).is_err()
        || f.read_exact(&mut banner).is_err()
    {
        return Some((code, Vec::new(), None));
    }

    let units: Vec<u16> = banner[TITLE_ENGLISH..TITLE_ENGLISH + 0x100]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    let title: Vec<String> = String::from_utf16_lossy(&units)
        .lines()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();

    Some((code, title, Some(decode_icon(&banner))))
}

/// 4x4 tiles of 8x8 pixels, 4 bits each, low nibble first. Palette entry 0 is transparent.
fn decode_icon(banner: &[u8]) -> Img {
    let mut pal = [[0u8; 4]; 16];
    for (i, p) in pal.iter_mut().enumerate().skip(1) {
        let c = u16::from_le_bytes([
            banner[ICON_PALETTE + i * 2],
            banner[ICON_PALETTE + i * 2 + 1],
        ]);
        let up = |v: u16| ((v & 31) as u8) << 3 | ((v & 31) as u8) >> 2;
        *p = [up(c), up(c >> 5), up(c >> 10), 255];
    }
    let mut img = Img::new(32, 32);
    for tile in 0..16 {
        for row in 0..8 {
            for col in 0..8 {
                let byte = banner[ICON_BITMAP + tile * 32 + row * 4 + col / 2];
                let idx = if col % 2 == 0 { byte & 15 } else { byte >> 4 } as usize;
                let (x, y) = ((tile % 4) * 8 + col, (tile / 4) * 8 + row);
                img.px[(y * 32 + x) * 4..][..4].copy_from_slice(&pal[idx]);
            }
        }
    }
    img
}

/// A GBA game: its name from the file (No-Intro names, less what is in brackets), with the
/// maker under it, and the game code from the cartridge header at 0xA0: a 12-character title,
/// the 4-character code and the 2-character maker. A zip is read up to the header of its first
/// file, which is all that has to come off the card.
fn parse_gba(path: &Path, stem: &str) -> (String, Vec<String>, Option<Img>) {
    let mut title = vec![tidy(stem)];
    let Some(head) = rom_head(path, 0xC0) else {
        return (String::new(), title, None);
    };
    let text = |r: std::ops::Range<usize>| -> String {
        head[r]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect()
    };
    let code = text(0xAC..0xB0);
    let code = if code.len() == 4 && code.chars().all(|c| c.is_ascii_alphanumeric()) {
        code
    } else {
        String::new()
    };
    if let Some(maker) = maker(&text(0xB0..0xB2)) {
        title.push(maker.to_owned());
    }
    if title[0].is_empty() {
        title[0] = text(0xA0..0xAC).trim().to_owned();
    }
    (code, title, None)
}

/// A GB or GBC game: as for GBA, the name from the file and the maker from the header, at
/// 0x100: the title from 0x134 (16 characters, or 11 and a 4-character game code before the CGB
/// flag at 0x143 on later carts), the licensee at 0x14B (or 0x144 when that says 0x33).
fn parse_gb(path: &Path, stem: &str) -> (String, Vec<String>, Option<Img>) {
    let mut title = vec![tidy(stem)];
    let Some(head) = rom_head(path, 0x150) else {
        return (String::new(), title, None);
    };
    let text = |r: std::ops::Range<usize>| -> String {
        head[r]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect()
    };
    let cgb = head[0x143] & 0x80 != 0;
    let code = text(0x13F..0x143);
    let code = if cgb
        && code.len() == 4
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        code
    } else {
        String::new()
    };
    let licensee = match head[0x14B] {
        0x33 => text(0x144..0x146),
        old => format!("{old:02X}"),
    };
    if let Some(maker) = maker(&licensee) {
        title.push(maker.to_owned());
    }
    if title[0].is_empty() {
        title[0] = text(0x134..if code.is_empty() { 0x143 } else { 0x13F })
            .trim()
            .to_owned();
    }
    (code, title, None)
}

/// The first `len` bytes of the ROM, from the file or from the first file in a zip.
fn rom_head(path: &Path, len: usize) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let zipped = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !zipped {
        let mut head = vec![0u8; len];
        f.read_exact(&mut head).ok()?;
        return Some(head);
    }
    // The local file header: signature, then method at 8, name and extra lengths at 26 and 28.
    let mut local = [0u8; 30];
    f.read_exact(&mut local).ok()?;
    if local[..4] != *b"PK\x03\x04" {
        return None;
    }
    let le16 = |at: usize| u16::from_le_bytes([local[at], local[at + 1]]) as u64;
    let (method, skip) = (le16(8), le16(26) + le16(28));
    f.seek(SeekFrom::Current(skip as i64)).ok()?;
    // Deflate reaches a header's worth of output well inside its first few kilobytes.
    let mut packed = vec![0u8; 16 * 1024];
    let n = f.read(&mut packed).ok()?;
    packed.truncate(n);
    let head = match method {
        0 => packed,
        8 => inflate_head(&packed, len)?,
        _ => return None,
    };
    (head.len() >= len).then(|| head[..len].to_vec())
}

/// The first `want` bytes a raw deflate stream inflates to, from as much of it as is given.
fn inflate_head(packed: &[u8], want: usize) -> Option<Vec<u8>> {
    use miniz_oxide::inflate::core::{decompress, inflate_flags, DecompressorOxide};
    let mut state = DecompressorOxide::new();
    let mut out = vec![0u8; 64 * 1024];
    let (_, _, made) = decompress(
        &mut state,
        packed,
        &mut out,
        0,
        inflate_flags::TINFL_FLAG_HAS_MORE_INPUT,
    );
    (made >= want).then(|| out[..want].to_vec())
}

/// "Advance Wars 2 - Black Hole Rising (USA)" is shown as "Advance Wars 2 - Black Hole Rising",
/// "Legend of Zelda, The - A Link to the Past (USA)" as "The Legend of Zelda - A Link to the Past".
fn tidy(stem: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.is_empty() {
        return stem.to_owned();
    }
    // No-Intro puts the article last: "King of Fighters EX, The - NeoBlood".
    let (head, tail) = out
        .split_once(" - ")
        .map_or((out.as_str(), ""), |(h, t)| (h, t));
    for article in ["The", "A", "An"] {
        if let Some(name) = head.strip_suffix(&format!(", {article}")) {
            let name = format!("{article} {name}");
            return if tail.is_empty() {
                name
            } else {
                format!("{name} - {tail}")
            };
        }
    }
    out
}

/// The publisher, by the two-character licensee code in a GBA header. The common ones only.
fn maker(code: &str) -> Option<&'static str> {
    Some(match code {
        "01" => "Nintendo",
        "08" => "Capcom",
        "13" => "Electronic Arts",
        "18" => "Hudson Soft",
        "20" => "Destination Software",
        "41" => "Ubisoft",
        "4F" => "Eidos",
        "4Q" => "Disney",
        "51" => "Acclaim",
        "52" => "Activision",
        "54" => "Take-Two",
        "5D" => "Midway",
        "5G" => "Majesco",
        "64" => "LucasArts",
        "69" => "Electronic Arts",
        "6S" => "TDK Mediactive",
        "70" => "Atari",
        "78" => "THQ",
        "7D" => "Vivendi",
        "7G" => "Jester Interactive",
        "8P" => "Sega",
        "A4" => "Konami",
        "AF" => "Namco",
        "B2" => "Bandai",
        "E9" => "Natsume",
        "EB" => "Atlus",
        "GD" => "Square Enix",
        "GT" => "505 Games",
        "HY" => "Sachen",
        "KM" => "Deep Silver",
        _ => return None,
    })
}

/// Mean colour of the icon's opaque pixels; the base for generated labels and boxes.
pub fn icon_colour(icon: &Img) -> [u8; 3] {
    let mut acc = [0u32; 3];
    let mut n = 0u32;
    for p in icon.px.chunks_exact(4).filter(|p| p[3] > 0) {
        for k in 0..3 {
            acc[k] += p[k] as u32;
        }
        n += 1;
    }
    if n == 0 {
        return [70, 80, 100];
    }
    [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]
}

/// Writes a Roms folder of tiny fake games (headers and DS banners only),
/// so the shelf can be tried without any.
pub fn make_demo(roms: &Path) -> std::io::Result<()> {
    const DEMO: [(&str, &str, &str, [&str; 8]); 6] = [
        (
            "Lantern Tide",
            "ALTE",
            "Harbor Works",
            [
                "..kkkk..", ".kyyyyk.", "kywyyyok", "kyyyyyok", "kyyyyook", ".koooyk.", "..kkkk..",
                "...kk...",
            ],
        ),
        (
            "Mole Patrol",
            "AMPE",
            "Burrow Soft",
            [
                "........", "..nnnn..", ".nnnnnn.", "nkwnnwkn", "nnnrrnnn", ".nnnnnn.", "..n..n..",
                "........",
            ],
        ),
        (
            "Kite & Compass",
            "AKCP",
            "Northwind",
            [
                "...r....", "..rrr...", ".rrwrr..", "rrwwwrr.", ".rrwrr..", "..rrr.k.", "...r...k",
                "......k.",
            ],
        ),
        (
            "Sunday Circuit",
            "ASCE",
            "Pitlane",
            [
                "........", "..rrrr..", ".rbbbbr.", "rrrrrrrr", "rwrrrrwr", "kkrrrrkk", "kk....kk",
                "........",
            ],
        ),
        (
            "Paper Harbor",
            "APHJ",
            "Foldworks",
            [
                "...k....", "...kw...", "...kww..", "...kwww.", "...k....", "nnnnnnnn", ".nnnnnn.",
                "bbbbbbbb",
            ],
        ),
        (
            "Vole Opera",
            "AVOE",
            "Burrow Soft",
            [
                "gggggggg", "g.g..g.g", "gggggggg", "..gyyg..", ".gyyyyg.", ".gywwyg.", ".gyyyyg.",
                "..gggg..",
            ],
        ),
    ];
    const PALETTE: [(char, [u8; 3]); 9] = [
        ('y', [255, 210, 122]),
        ('o', [232, 137, 47]),
        ('w', [255, 244, 214]),
        ('k', [16, 19, 26]),
        ('b', [121, 200, 228]),
        ('r', [226, 59, 59]),
        ('g', [123, 191, 106]),
        ('n', [138, 86, 40]),
        ('.', [0, 0, 0]),
    ];
    let dir = roms.join(Console::Ds.folder());
    fs::create_dir_all(&dir)?;
    for (name, code, maker, art) in DEMO {
        let mut rom = vec![0u8; 0x200 + BANNER_LEN];
        rom[..name.len().min(12)]
            .copy_from_slice(&name.to_ascii_uppercase().as_bytes()[..name.len().min(12)]);
        rom[0x0C..0x10].copy_from_slice(code.as_bytes());
        rom[0x68..0x6C].copy_from_slice(&0x200u32.to_le_bytes());
        let b = &mut rom[0x200..];
        b[0] = 1;
        for (i, (_, c)) in PALETTE.iter().enumerate().take(8) {
            let v = (c[0] as u16 >> 3) | (c[1] as u16 >> 3) << 5 | (c[2] as u16 >> 3) << 10;
            b[ICON_PALETTE + (i + 1) * 2..][..2].copy_from_slice(&v.to_le_bytes());
        }
        for y in 0..32 {
            for x in 0..32 {
                let ch = art[y / 4].as_bytes()[x / 4] as char;
                let idx = PALETTE
                    .iter()
                    .position(|(p, _)| *p == ch)
                    .map(|i| (i + 1) % 9)
                    .unwrap_or(0) as u8;
                let (tile, row, col) = ((y / 8) * 4 + x / 8, y % 8, x % 8);
                b[ICON_BITMAP + tile * 32 + row * 4 + col / 2] |=
                    if col % 2 == 0 { idx } else { idx << 4 };
            }
        }
        let text = format!("{name}\n{maker}");
        for lang in 0..6 {
            for (i, u) in text.encode_utf16().enumerate() {
                b[0x240 + lang * 0x100 + i * 2..][..2].copy_from_slice(&u.to_le_bytes());
            }
        }
        fs::write(dir.join(format!("{name}.nds")), rom)?;
    }
    let gba = roms.join(Console::Gba.folder());
    fs::create_dir_all(&gba)?;
    for (name, code, maker) in [
        ("Tidepool Racer (USA)", "ATRE", "78"),
        ("Moth & Lamp (Europe)", "AMLP", "01"),
        ("Signal Hill (Japan)", "ASHJ", "A4"),
    ] {
        let mut rom = vec![0u8; 0xC0];
        let short = name.to_ascii_uppercase();
        let short = &short.as_bytes()[..12];
        rom[0xA0..0xAC].copy_from_slice(short);
        rom[0xAC..0xB0].copy_from_slice(code.as_bytes());
        rom[0xB0..0xB2].copy_from_slice(maker.as_bytes());
        rom[0xB2] = 0x96;
        fs::write(gba.join(format!("{name}.gba")), rom)?;
    }
    // Game Boy games, original and Color-only: a header with a title, and a code on the
    // Color one.
    for (console, name, code) in [
        (Console::Gb, "Pebble Garden (World)", ""),
        (Console::Gb, "Lighthouse Keeper (USA, Europe)", ""),
        (Console::Gbc, "Firefly Market (USA)", "AFME"),
        (Console::Gbc, "Copper Comet (Europe)", "ACCP"),
    ] {
        let dir = roms.join(console.folder());
        fs::create_dir_all(&dir)?;
        let mut rom = vec![0u8; 0x150];
        let title = name.to_ascii_uppercase();
        let title = &title.as_bytes()[..11];
        rom[0x134..0x13F].copy_from_slice(title);
        if !code.is_empty() {
            rom[0x13F..0x143].copy_from_slice(code.as_bytes());
            rom[0x143] = 0xC0;
        }
        rom[0x14B] = 0x01;
        fs::write(dir.join(format!("{name}.{}", console.ext())), rom)?;
    }
    // SNES games: fabricated LoROM headers, not executable ROMs.
    let snes = roms.join(Console::Snes.folder());
    fs::create_dir_all(&snes)?;
    for (name, code) in [
        ("Moon Bay (USA)", "AMBE"),
        ("Cloud Orchard (Japan)", "ACOJ"),
    ] {
        let mut rom = vec![0u8; 0x8000];
        let head = &mut rom[0x7FB0..];
        head[..2].copy_from_slice(b"01");
        head[2..6].copy_from_slice(code.as_bytes());
        head[0x10..0x25].fill(b' ');
        head[0x10..0x18].copy_from_slice(b"DEMO ROM");
        head[0x25] = 0x20;
        head[0x2A] = 0x33;
        head[0x2C..0x30].copy_from_slice(&[0xCB, 0xED, 0x34, 0x12]);
        head[0x4C..0x4E].copy_from_slice(&0x8000u16.to_le_bytes());
        fs::write(snes.join(format!("{name}.sfc")), rom)?;
    }
    // A fabricated iNES cartridge header for the NES shelf preview; not a playable game.
    let nes = roms.join(Console::Nes.folder());
    fs::create_dir_all(&nes)?;
    for name in ["Star Lantern (World)", "Paper Harbor (USA)"] {
        let mut rom = vec![0; 16];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[5] = 1;
        fs::write(nes.join(format!("{name}.nes")), rom)?;
    }
    // Named placeholders for shelf previews; no executable game data.
    for (console, names) in [
        (
            Console::MegaDrive,
            ["Copper Coast (World)", "Harbour Run (USA)"],
        ),
        (
            Console::GameGear,
            ["Pocket Orchard (World)", "Lantern Ferry (Europe)"],
        ),
        (Console::Lynx, ["Amber Trail (World)", "Night Glider (USA)"]),
    ] {
        let folder = roms.join(console.folder());
        fs::create_dir_all(&folder)?;
        for name in names {
            fs::write(
                folder.join(format!("{name}.{}", console.ext())),
                b"DSlot preview",
            )?;
        }
    }
    // An app, with an icon: a folder in APPS (nibkit::dslot::apps).
    let app = roms.join("APPS/Tide Clock");
    fs::create_dir_all(&app)?;
    let toml = "name = \"Tide Clock\"\ndescription = \"A demo app\"\ncommand = [\"sleep\", \"1\"]\nruns_on = [\"nibos\", \"stock\", \"desktop\"]\n";
    fs::write(app.join("app.toml"), toml)?;
    let mut icon = Img::new(64, 64);
    icon.rrect(0.0, 0.0, 64.0, 64.0, [12.0; 4], [40, 120, 150, 255]);
    icon.rrect(18.0, 18.0, 28.0, 28.0, [14.0; 4], [255, 210, 122, 255]);
    crate::gfx::save_png(&app.join("icon.png"), 64, 64, &icon.px).map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn names(games: &[Game]) -> Vec<&str> {
        games.iter().map(|g| g.name()).collect()
    }

    /// Two cards, one laid out as the stock OS does (`Roms/`), the other set up for slot CFW
    /// (`Games/`): both are read, and a game in both is listed once, as the one in `Roms/`.
    #[test]
    fn slot_card_layout() {
        let dir = TempDir::new("slot-layout");
        let put = |path: &str| {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b"fixture").unwrap();
            p
        };
        put("one/Roms/GB/Tetris (World).gb");
        put("two/Games/GB/Tetris (World).gb");
        put("two/Games/GB/Kirby's Dream Land (USA, Europe).gb");
        put("one/Games/GB/Link's Awakening.gb");
        let dirs: Vec<PathBuf> = ["one/Roms", "two/Roms", "one/Games", "two/Games"]
            .iter()
            .map(|r| dir.path().join(r).join(Console::Gb.folder()))
            .collect();
        let cache = dir.path().join(Console::Gb.known());
        let mut found: Vec<PathBuf> = scan(Console::Gb, &dirs, &cache)
            .into_iter()
            .map(|g| g.path.strip_prefix(dir.path()).unwrap().to_path_buf())
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                "one/Games/GB/Link's Awakening.gb",
                "one/Roms/GB/Tetris (World).gb",
                "two/Games/GB/Kirby's Dream Land (USA, Europe).gb",
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn disc_images_and_the_newer_handhelds() {
        let dir = TempDir::new("cd-and-handhelds");
        let put = |folder: &str, names: &[&str]| {
            let at = dir.path().join(folder);
            fs::create_dir_all(&at).unwrap();
            for name in names {
                fs::write(at.join(name), b"fixture").unwrap();
            }
            at
        };
        // A CD game is its cue sheet or a CHD: the tracks a cue lists are not games of their
        // own, and a zip is not taken.
        let cd = put(
            "PCECD",
            &[
                "Ys Book I & II (Japan).cue",
                "Ys Book I & II (Japan) (Track 01).bin",
                "Ys Book I & II (Japan) (Track 02).bin",
                "Gate of Thunder (USA).chd",
                "Other.zip",
            ],
        );
        let cache = dir.path().join(Console::PcEngineCd.known());
        let names = |games: Vec<Game>| -> Vec<String> {
            games.iter().map(|g| g.name().to_owned()).collect()
        };
        assert_eq!(
            names(scan(Console::PcEngineCd, std::slice::from_ref(&cd), &cache)),
            ["Gate of Thunder", "Ys Book I & II"]
        );
        let ngp = put(
            "NGP",
            &[
                "Puzzle Link (Europe).ngp",
                "Card Fighters (USA).ngc",
                "a.zip",
            ],
        );
        let cache = dir.path().join(Console::NeoGeoPocket.known());
        assert_eq!(
            scan(Console::NeoGeoPocket, std::slice::from_ref(&ngp), &cache).len(),
            3
        );
        let ws = put(
            "WS",
            &["Gunpey (Japan).ws", "Judgement Silversword (Japan).wsc"],
        );
        let cache = dir.path().join(Console::WonderSwan.known());
        assert_eq!(
            scan(Console::WonderSwan, std::slice::from_ref(&ws), &cache).len(),
            2
        );
        // A playlist is one game of several discs: the discs it lists aren't games of their own.
        let ps = put(
            "PS",
            &[
                "Gran Turismo 2 (USA) (Disc 1).chd",
                "Gran Turismo 2 (USA) (Disc 2).chd",
                "Gran Turismo (USA).chd",
                "Ape Escape (USA).cue",
                "Ape Escape (USA).bin",
            ],
        );
        fs::write(
            ps.join("Gran Turismo 2 (USA).m3u"),
            "# both discs\nGran Turismo 2 (USA) (Disc 1).chd\nGran Turismo 2 (USA) (Disc 2).chd\n",
        )
        .unwrap();
        let cache = dir.path().join(Console::PlayStation.known());
        assert_eq!(
            names(scan(
                Console::PlayStation,
                std::slice::from_ref(&ps),
                &cache
            )),
            ["Ape Escape", "Gran Turismo", "Gran Turismo 2"]
        );
        // Outside their folders, the extensions say whose they are.
        let console = |name: &str| read_one(&dir.path().join(name)).map(|g| g.console);
        fs::write(dir.path().join("x.wsc"), b"fixture").unwrap();
        fs::write(dir.path().join("x.chd"), b"fixture").unwrap();
        fs::write(dir.path().join("x.fds"), b"fixture").unwrap();
        assert_eq!(console("x.wsc"), Some(Console::WonderSwan));
        assert_eq!(console("x.chd"), Some(Console::PcEngineCd));
        assert_eq!(console("x.fds"), Some(Console::Fds));
        assert_eq!(Console::from_folder("SMS"), Some(Console::MasterSystem));
    }

    /// MAME sets (zipped or 7-Zipped), named by the list an emulator gives (`[games]`).
    #[test]
    fn game_and_watch_sets_named_by_a_list() {
        let dir = TempDir::new("gamewatch-sets");
        let folder = dir.path().join("GW");
        fs::create_dir(&folder).unwrap();
        for name in ["gnw_dkjr.7z", "gnw_ball.zip", "Parachute.mgw"] {
            fs::write(folder.join(name), b"fixture").unwrap();
        }
        let list = dir.path().join("units.tsv");
        fs::write(
            &list,
            "gnw_dkjr\tDonkey Kong Jr.\tDJ-101\t1982\tNew Wide Screen\ngnw_ball\tBall\n",
        )
        .unwrap();
        let text = format!(
            "name = \"inktick\"\n[systems]\nGW = [\"/usr/bin/inktick\", \"{{rom}}\"]\n[games]\nGW = \"{}\"\n",
            list.display()
        );
        let emulators =
            crate::emulator::Emulators::assume_installed([("inktick".to_owned(), text)]);
        let cache = dir.path().join(Console::GameWatch.known());
        let mut games = scan(Console::GameWatch, std::slice::from_ref(&folder), &cache);
        assert_eq!(games.len(), 3);
        name_from_lists(&mut games, &emulators);
        let dkjr = games.iter().find(|g| g.stem == "gnw_dkjr").unwrap();
        assert_eq!(
            dkjr.title,
            [
                "Donkey Kong Jr.",
                "DJ-101  \u{b7}  1982  \u{b7}  New Wide Screen"
            ]
        );
        let ball = games.iter().find(|g| g.stem == "gnw_ball").unwrap();
        assert_eq!(ball.title, ["Ball"]);
        // Not listed: named by its file, as before.
        assert!(games.iter().any(|g| g.name() == "Parachute"));
    }

    #[test]
    fn game_and_watch_units_by_their_file_names() {
        let dir = TempDir::new("gamewatch");
        let folder = dir.path().join("GW");
        fs::create_dir(&folder).unwrap();
        for name in [
            "Donkey Kong (Nintendo, Multi Screen).mgw",
            "Fire (Nintendo, Wide Screen).mgw.zip",
            "Parachute.MGW",
            "notes.txt",
        ] {
            fs::write(folder.join(name), b"fixture").unwrap();
        }
        let cache = dir.path().join(Console::GameWatch.known());
        let games = scan(Console::GameWatch, std::slice::from_ref(&folder), &cache);
        let names: Vec<&str> = games.iter().map(Game::name).collect();
        assert_eq!(names, ["Donkey Kong", "Fire", "Parachute"]);
        assert!(games
            .iter()
            .all(|g| g.serial().is_empty() && g.icon.is_none()));
        let game = read_one(&folder.join("Fire (Nintendo, Wide Screen).mgw.zip")).unwrap();
        assert_eq!(
            (game.console, game.stem.as_str()),
            (Console::GameWatch, "Fire (Nintendo, Wide Screen)")
        );
        assert_eq!(Console::from_folder("GW"), Some(Console::GameWatch));
    }

    #[test]
    fn sega_and_lynx_folders_archives_and_cache() {
        for (console, names) in [
            (
                Console::MegaDrive,
                vec![
                    "Coast (World).md",
                    "Harbour.gen.zip",
                    "Moss.smd",
                    "River.bin",
                ],
            ),
            (
                Console::GameGear,
                vec!["Coast (World).GG", "Harbour.gg.zip"],
            ),
            (Console::Lynx, vec!["Coast (World).lnx", "Harbour.lnx.zip"]),
        ] {
            let dir = TempDir::new(console.key());
            let folder = dir.path().join(console.folder());
            fs::create_dir(&folder).unwrap();
            for name in &names {
                fs::write(folder.join(name), b"fixture").unwrap();
            }
            fs::write(folder.join("ignore.txt"), b"not a cartridge").unwrap();
            let cache = dir.path().join(console.known());
            let games = scan(console, std::slice::from_ref(&folder), &cache);
            assert_eq!(games.len(), names.len());
            assert!(games
                .iter()
                .all(|g| g.console == console && g.code.is_empty() && g.serial().is_empty()));
            assert_eq!(
                scan(console, std::slice::from_ref(&folder), &cache).len(),
                names.len()
            );
            assert_eq!(
                games
                    .iter()
                    .find(|g| g.stem == "Coast (World)")
                    .unwrap()
                    .name(),
                "Coast"
            );
            for name in names {
                let game = read_one(&folder.join(name)).unwrap();
                assert_eq!(game.console, console);
                assert!(!game.stem.ends_with(&format!(".{}", console.ext())));
            }
        }
    }

    #[test]
    fn nes_games_names_and_cache() {
        let dir = TempDir::new("nes-games");
        let folder = dir.path().join("FC");
        fs::create_dir(&folder).unwrap();
        for name in [
            "Star Lantern (World).nes",
            "Paper Harbor.NES",
            "Red Boat.unf",
            "Green Bay.unif.zip",
        ] {
            fs::write(folder.join(name), b"NES\x1a").unwrap();
        }
        fs::write(folder.join("music.nsf"), b"not a cartridge").unwrap();
        fs::write(folder.join("disk.fds"), b"not a cartridge").unwrap();
        let cache = dir.path().join(Console::Nes.known());
        let games = scan(Console::Nes, std::slice::from_ref(&folder), &cache);
        assert_eq!(games.len(), 4);
        assert!(games
            .iter()
            .all(|g| g.console == Console::Nes && g.code.is_empty() && g.serial().is_empty()));
        let star = games
            .iter()
            .find(|g| g.stem == "Star Lantern (World)")
            .unwrap();
        assert_eq!(star.name(), "Star Lantern");
        assert_eq!(star.key(), "FC/Star Lantern (World)");
        assert_eq!(scan(Console::Nes, &[folder], &cache).len(), 4);
        for name in ["game.nes", "game.unf", "game.unif.zip", "game.NES.ZIP"] {
            let g = read_one(&dir.path().join(name)).unwrap();
            assert_eq!(g.console, Console::Nes);
            assert_eq!(g.stem, "game");
        }
    }

    #[test]
    fn snes_games_names_and_cache() {
        let dir = TempDir::new("rom-snes");
        make_demo(&dir.path().join("Roms")).unwrap();
        let snes = dir.path().join("Roms/SFC");
        fs::write(snes.join("Harbor (Europe).SMC"), b"bad header").unwrap();
        fs::write(snes.join("Moss.sfc.zip"), b"bad zip").unwrap();
        fs::write(snes.join(".hidden.sfc"), b"").unwrap();
        fs::write(snes.join("notes.txt"), b"").unwrap();
        let cache = dir.path().join(Console::Snes.known());
        let games = scan(Console::Snes, std::slice::from_ref(&snes), &cache);
        assert_eq!(
            names(&games),
            ["Cloud Orchard", "Harbor", "Moon Bay", "Moss"]
        );
        assert_eq!(games[0].serial(), "SHVC-ACOJ-JPN");
        assert_eq!(games[2].serial(), "SNS-AMBE-USA");
        assert_eq!(games[2].title[1], "Nintendo");
        assert_eq!(games[3].art("Imgs"), snes.join("Imgs/Moss.png"));
        assert_eq!(games[3].key(), "SFC/Moss");
        fs::write(&games[2].path, b"changed since scan").unwrap();
        let again = scan(Console::Snes, std::slice::from_ref(&snes), &cache);
        assert_eq!(again[2].code, "AMBE");
        for name in ["Moon.sfc", "Moon.SMC", "Moon.smc.zip", "Moon.sfc.ZIP"] {
            let game = read_one(&dir.path().join(name)).unwrap();
            assert_eq!(game.console, Console::Snes, "{name}");
            assert_eq!(game.stem, "Moon", "{name}");
        }
    }

    #[test]
    fn ds_games_and_the_known_file() {
        let dir = TempDir::new("rom-ds");
        make_demo(&dir.path().join("Roms")).unwrap();
        let (roms, known) = (
            vec![dir.path().join("Roms/NDS")],
            dir.path().join("cache/games.bin"),
        );
        let games = scan(Console::Ds, &roms, &known);
        // The banner's title, sorted by name.
        assert_eq!(
            names(&games),
            [
                "Kite & Compass",
                "Lantern Tide",
                "Mole Patrol",
                "Paper Harbor",
                "Sunday Circuit",
                "Vole Opera"
            ]
        );
        let lantern = &games[1];
        assert_eq!(
            (lantern.code.as_str(), lantern.serial().as_str()),
            ("ALTE", "NTR-ALTE-USA")
        );
        assert_eq!(lantern.title[1], "Harbor Works");
        assert!(lantern
            .icon
            .as_ref()
            .is_some_and(|i| i.w == 32 && i.h == 32));
        assert!(known.is_file());

        // A ROM known from before isn't read again: its title comes from the known file.
        fs::write(&lantern.path, b"not a ROM any more").unwrap();
        assert_eq!(scan(Console::Ds, &roms, &known)[1].name(), "Lantern Tide");
        // One gone is gone from the list, and from the file.
        fs::remove_file(&lantern.path).unwrap();
        assert_eq!(scan(Console::Ds, &roms, &known).len(), 5);
        assert_eq!(read_known(&known).unwrap().len(), 5);

        // A zipped DS game is listed by its file name; other files are not listed.
        fs::write(roms[0].join("Zebra Sprint (Europe).zip"), b"PK").unwrap();
        fs::write(roms[0].join("notes.txt"), b"").unwrap();
        fs::write(roms[0].join(".hidden.nds"), b"").unwrap();
        let games = scan(Console::Ds, &roms, &known);
        assert_eq!(
            games.last().map(|g| g.name()),
            Some("Zebra Sprint (Europe)")
        );
        assert_eq!(games.len(), 6);
    }

    #[test]
    fn gba_games() {
        let dir = TempDir::new("rom-gba");
        make_demo(&dir.path().join("Roms")).unwrap();
        let gba = dir.path().join("Roms/GBA");
        // A zipped one, stored and deflated: only the start of its first file is read.
        let header = fs::read(gba.join("Tidepool Racer (USA).gba")).unwrap();
        fs::write(gba.join("Advance Rally (USA).zip"), zip(&header, true)).unwrap();
        fs::write(gba.join("Bright Lake (Japan).zip"), zip(&header, false)).unwrap();
        let games = scan(Console::Gba, &[gba], &dir.path().join("gba.bin"));
        assert_eq!(
            names(&games),
            [
                "Advance Rally",
                "Bright Lake",
                "Moth & Lamp",
                "Signal Hill",
                "Tidepool Racer"
            ]
        );
        let tidepool = &games[4];
        assert_eq!(
            (tidepool.serial().as_str(), tidepool.title[1].as_str()),
            ("AGB-ATRE-USA", "THQ")
        );
        assert_eq!(games[0].code, "ATRE");
        assert_eq!(games[1].code, "ATRE");
    }

    #[test]
    fn gb_games() {
        let dir = TempDir::new("rom-gb-games");
        let gb = dir.path().join("Roms/GB");
        fs::create_dir_all(&gb).unwrap();
        let mut rom = vec![0u8; 0x150];
        rom[0x134..0x13F].copy_from_slice(b"TETRIS DX\0\0");
        rom[0x13F..0x143].copy_from_slice(b"ATXE");
        rom[0x143] = 0x80;
        rom[0x14B] = 0x01;
        fs::write(gb.join("Prehistorik Man (USA, Europe).gb"), &rom).unwrap();
        // A Color game in the GB folder, zipped with its own extension kept.
        let zipped = gb.join("Tetris DX (World) (SGB Enhanced) (GB Compatible).gbc.zip");
        fs::write(&zipped, zip(&rom, true)).unwrap();
        fs::write(gb.join("Bubble Bobble (USA, Europe).zip"), zip(&rom, false)).unwrap();
        fs::write(gb.join("notes.txt"), "not a game").unwrap();
        let games = scan(
            Console::Gb,
            std::slice::from_ref(&gb),
            &dir.path().join("gb.bin"),
        );
        assert_eq!(
            names(&games),
            ["Bubble Bobble", "Prehistorik Man", "Tetris DX"]
        );
        let tetris = &games[2];
        // Its artwork and saves go by the name without .gbc.zip.
        assert_eq!(
            tetris.stem,
            "Tetris DX (World) (SGB Enhanced) (GB Compatible)"
        );
        assert_eq!(
            tetris.art("Imgs"),
            gb.join("Imgs/Tetris DX (World) (SGB Enhanced) (GB Compatible).png")
        );
        assert_eq!(tetris.code, "ATXE");

        // Outside a console's folder, a zip is the console of the extension under it.
        let loose = dir.path().join("Tetris DX (World).gbc.zip");
        fs::copy(&zipped, &loose).unwrap();
        let one = read_one(&loose).unwrap();
        assert_eq!(
            (one.console, one.stem.as_str()),
            (Console::Gbc, "Tetris DX (World)")
        );
        assert_eq!(
            rom_name(Path::new("Roms/NDS/Fossil (Europe).nds.zip")).as_deref(),
            Some("Fossil (Europe)")
        );
        assert_eq!(
            rom_name(Path::new("Roms/GB/Dr. Mario (World).zip")).as_deref(),
            Some("Dr. Mario (World)")
        );
    }

    /// A zip holding `data` as its first file, deflated or stored.
    fn zip(data: &[u8], deflate: bool) -> Vec<u8> {
        let packed = if deflate {
            miniz_oxide::deflate::compress_to_vec(data, 6)
        } else {
            data.to_vec()
        };
        let name = b"game.gba";
        let mut out = b"PK\x03\x04".to_vec();
        out.extend([20, 0, 0, 0]);
        out.extend((if deflate { 8u16 } else { 0 }).to_le_bytes());
        // Modification time and date, CRC (not checked).
        out.extend([0; 8]);
        out.extend((packed.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0, 0]);
        out.extend(name);
        out.extend(packed);
        out
    }

    #[test]
    fn demo_app() {
        let dir = TempDir::new("rom-apps");
        let roms = dir.path().join("Roms");
        make_demo(&roms).unwrap();
        let found = nibkit::dslot::apps(std::slice::from_ref(&roms));
        let games = apps(&found);
        assert_eq!(games.len(), 1);
        let app = &games[0];
        assert_eq!(
            (app.console, app.name(), app.title[1].as_str()),
            (Console::Apps, "Tide Clock", "A demo app")
        );
        assert_eq!(app.path, roms.join("APPS/Tide Clock"));
        assert_eq!(app.art("Imgs"), roms.join("APPS/Tide Clock/icon.png"));
        assert_eq!(app.key(), "APPS/Tide Clock");
        assert_eq!(app_path(&found[0]), app.path);
    }

    #[test]
    fn tidy_names() {
        assert_eq!(
            tidy("Advance Wars 2 - Black Hole Rising (USA)"),
            "Advance Wars 2 - Black Hole Rising"
        );
        assert_eq!(
            tidy("Legend of Zelda, The - A Link to the Past (USA)"),
            "The Legend of Zelda - A Link to the Past"
        );
        assert_eq!(tidy("Urbz, The (Europe) (En,Fr,De) [!]"), "The Urbz");
        assert_eq!(tidy("Sims, An Odd Name"), "Sims, An Odd Name");
        // Nothing left once the brackets are gone: the file's name as it is.
        assert_eq!(tidy("(Test Cart)"), "(Test Cart)");
    }

    /// A CGB cart's header: an 11-character title, the game code, the CGB flag and a new licensee.
    #[test]
    fn gb_header() {
        let dir = TempDir::new("rom-gb");
        let mut rom = vec![0u8; 0x150];
        rom[0x134..0x13F].copy_from_slice(b"POKEMON_SLV");
        rom[0x13F..0x143].copy_from_slice(b"AAXE");
        rom[0x143] = 0x80;
        rom[0x144..0x146].copy_from_slice(b"01");
        rom[0x14B] = 0x33;
        let path = dir
            .path()
            .join("Pokemon - Silver Version (USA, Europe) (SGB Enhanced) (GB Compatible).gbc");
        fs::write(&path, &rom).unwrap();
        let (code, title, _) = parse_gb(&path, path.file_stem().unwrap().to_str().unwrap());
        assert_eq!(code, "AAXE");
        assert_eq!(title, ["Pokemon - Silver Version", "Nintendo"]);

        // An original DMG cart: 16-character title, old licensee byte, no code.
        let mut rom = vec![0u8; 0x150];
        rom[0x134..0x13D].copy_from_slice(b"TETRIS\0\0\0");
        rom[0x14B] = 0x01;
        fs::write(&path, &rom).unwrap();
        let (code, title, _) = parse_gb(&path, "");
        assert_eq!(code, "");
        assert_eq!(title, ["TETRIS", "Nintendo"]);
    }
}
