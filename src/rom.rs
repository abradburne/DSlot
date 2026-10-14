//! Reads what a DS ROM says about itself: game code, banner title and the 32x32 icon.

use crate::gfx::Img;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct Game {
    pub path: PathBuf,
    pub stem: String,
    /// Four-character game code from the header, e.g. `ALTE`. Empty when unreadable.
    pub code: String,
    /// Banner title split into lines; the first is the game's name.
    pub title: Vec<String>,
    pub icon: Option<Img>,
}

impl Game {
    pub fn name(&self) -> &str {
        &self.title[0]
    }

    /// `NTR-ALTE-USA`, as printed on a real card.
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
        format!("NTR-{}{}", self.code, region)
    }

    /// `<rom dir>/<folder>/<rom name>.png`
    pub fn art(&self, folder: &str) -> PathBuf {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        dir.join(folder).join(format!("{}.png", self.stem))
    }
}

/// `known` remembers what each ROM said the last time, so that a start only lists the folders
/// and opens the ROMs it has not met. A ROM is known by its path alone: asking the card for
/// each file's size and date costs as much as reading it. Deleting the file has every ROM
/// read again.
pub fn scan(dirs: &[PathBuf], known: &Path) -> Vec<Game> {
    let old = read_known(known).unwrap_or_default();
    crate::mark("known games read");
    let mut fresh = false;
    let mut games = Vec::new();
    let mut roms = 0;
    for dir in dirs {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else { continue };
            if stem.starts_with('.') || !matches!(ext.as_str(), "nds" | "zip") {
                continue;
            }
            if ext != "nds" {
                games.push(Game { path, title: vec![stem.clone()], stem, code: String::new(), icon: None });
                continue;
            }
            roms += 1;
            let (code, title, icon) = match old.get(&path) {
                Some(k) => (k.code.clone(), k.title.clone(), k.icon.clone()),
                None => {
                    fresh = true;
                    parse(&path).unwrap_or_default()
                }
            };
            let title = if title.is_empty() { vec![stem.clone()] } else { title };
            games.push(Game { path, stem, code, title, icon });
        }
    }
    // Rewritten when a ROM was read, and when one has gone.
    if fresh || roms != old.len() {
        if let Err(e) = write_known(known, &games) {
            eprintln!("dslot: cannot write {}: {e}", known.display());
        }
    }
    games.sort_by_key(|g| g.name().to_lowercase());
    games
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
    for g in games.iter().filter(|g| g.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("nds"))) {
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
            _ => Some(Img { w: 32, h: 32, px: take(&mut rest, 32 * 32 * 4)?.to_vec() }),
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
    let code = if code.chars().all(|c| c.is_ascii_alphanumeric()) { code } else { String::new() };

    let offset = u32::from_le_bytes(head[0x68..0x6C].try_into().ok()?);
    let mut banner = [0u8; BANNER_LEN];
    if offset == 0 || f.seek(SeekFrom::Start(offset as u64)).is_err() || f.read_exact(&mut banner).is_err() {
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
        let c = u16::from_le_bytes([banner[ICON_PALETTE + i * 2], banner[ICON_PALETTE + i * 2 + 1]]);
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

/// Write a handful of tiny fake ROMs (header + banner only) so the UI can be tried without games.
pub fn make_demo(dir: &Path) -> std::io::Result<()> {
    const DEMO: [(&str, &str, &str, [&str; 8]); 6] = [
        ("Lantern Tide", "ALTE", "Harbor Works", ["..kkkk..", ".kyyyyk.", "kywyyyok", "kyyyyyok", "kyyyyook", ".koooyk.", "..kkkk..", "...kk..."]),
        ("Mole Patrol", "AMPE", "Burrow Soft", ["........", "..nnnn..", ".nnnnnn.", "nkwnnwkn", "nnnrrnnn", ".nnnnnn.", "..n..n..", "........"]),
        ("Kite & Compass", "AKCP", "Northwind", ["...r....", "..rrr...", ".rrwrr..", "rrwwwrr.", ".rrwrr..", "..rrr.k.", "...r...k", "......k."]),
        ("Sunday Circuit", "ASCE", "Pitlane", ["........", "..rrrr..", ".rbbbbr.", "rrrrrrrr", "rwrrrrwr", "kkrrrrkk", "kk....kk", "........"]),
        ("Paper Harbor", "APHJ", "Foldworks", ["...k....", "...kw...", "...kww..", "...kwww.", "...k....", "nnnnnnnn", ".nnnnnn.", "bbbbbbbb"]),
        ("Vole Opera", "AVOE", "Burrow Soft", ["gggggggg", "g.g..g.g", "gggggggg", "..gyyg..", ".gyyyyg.", ".gywwyg.", ".gyyyyg.", "..gggg.."]),
    ];
    const PALETTE: [(char, [u8; 3]); 9] = [
        ('y', [255, 210, 122]), ('o', [232, 137, 47]), ('w', [255, 244, 214]), ('k', [16, 19, 26]),
        ('b', [121, 200, 228]), ('r', [226, 59, 59]), ('g', [123, 191, 106]), ('n', [138, 86, 40]), ('.', [0, 0, 0]),
    ];
    fs::create_dir_all(dir)?;
    for (name, code, maker, art) in DEMO {
        let mut rom = vec![0u8; 0x200 + BANNER_LEN];
        rom[..name.len().min(12)].copy_from_slice(&name.to_ascii_uppercase().as_bytes()[..name.len().min(12)]);
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
                let idx = PALETTE.iter().position(|(p, _)| *p == ch).map(|i| (i + 1) % 9).unwrap_or(0) as u8;
                let (tile, row, col) = ((y / 8) * 4 + x / 8, y % 8, x % 8);
                b[ICON_BITMAP + tile * 32 + row * 4 + col / 2] |= if col % 2 == 0 { idx } else { idx << 4 };
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
    Ok(())
}
