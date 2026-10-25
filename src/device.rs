//! The Apps page's card: in the same frame as a system's card, but about the device in your
//! hands. What it is (its board's facts, `data/devices/<board>.toml`), its system and when that
//! was built, and how it is now: its battery (the charge, and whether battery protect is on),
//! the room left on each card, how long it has been on, and what has been played on it. What
//! the system doesn't say (the stock OS keeps the battery to itself; a desktop is no board) is
//! left out.

use crate::rom::{Console, Game};
use crate::stats::Stats;
use crate::systems::Stat;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The device's card.
pub struct About {
    /// The device: "Anbernic RG DS+".
    pub name: String,
    /// Its system and when that was built.
    pub system: String,
    /// What it is made of, by its board's facts: chip, CPU, GPU, memory, screens, storage.
    pub specs: &'static [Spec],
    /// Where those come from.
    pub source: &'static str,
    /// How it is now, the shares (charge, room) with a bar.
    pub now: Vec<Stat>,
    /// What is on the cards, in a sentence.
    pub library: String,
    /// The game played longest of all, with its system and time.
    pub most: Option<String>,
}

/// A board's facts.
#[derive(Deserialize)]
struct Board {
    board: String,
    name: String,
    source: String,
    specs: Vec<Spec>,
}

#[derive(Deserialize)]
pub struct Spec {
    pub label: String,
    pub value: String,
}

/// The boards there are facts for: built into DSlot.
const BOARDS: [&str; 1] = [include_str!("../data/devices/rg-ds-plus.toml")];

/// This device's board: nibOS's board file says which; the stock OS only runs on the RG DS+;
/// a desktop preview is none.
fn board() -> Option<&'static Board> {
    static BOARD: OnceLock<Option<Board>> = OnceLock::new();
    BOARD
        .get_or_init(|| {
            let id = if cfg!(feature = "stock") {
                Some("rg-ds-plus".to_owned())
            } else {
                std::fs::read_to_string("/usr/lib/nibos/board.conf")
                    .ok()
                    .and_then(|t| {
                        fields(&t)
                            .find(|&(k, _)| k == "NIBOS_BOARD")
                            .map(|(_, v)| v.to_owned())
                    })
            }?;
            BOARDS
                .iter()
                .filter_map(|t| toml::from_str::<Board>(t).ok())
                .find(|b| b.board == id)
        })
        .as_ref()
}

/// The card now. The board and system are read once; the battery and the cards every few
/// seconds.
pub fn about(games: &[Game], stats: &Stats) -> About {
    let mut now = Vec::new();
    let r = readings();
    if let Some((percent, state)) = &r.battery {
        now.push(Stat {
            label: "Battery".into(),
            value: format!("{percent}%{state}"),
            bar: None,
            share: Some(*percent as f64 / 100.0),
        });
    }
    for (n, space) in r.space.iter().enumerate() {
        now.push(Stat {
            label: format!("Card {} free", n + 1),
            value: format!("{} of {}", size(space.free), size(space.total)),
            bar: None,
            share: Some(space.free as f64 / space.total.max(1) as f64),
        });
    }
    if let Some(on) = r.protect {
        let value = if on {
            format!("On, to about {}%", nibkit::battery::PROTECT_PERCENT)
        } else {
            "Off".to_owned()
        };
        now.push(stat("Battery protect", value));
    }
    if let Some(up) = uptime() {
        now.push(stat("On for", span(up.as_secs())));
    }

    // What is on the cards, and what has been played.
    let here: Vec<&Game> = games.iter().filter(|g| !g.gone).collect();
    let consoles = |gs: &[&Game]| {
        let mut cs: Vec<Console> = gs
            .iter()
            .map(|g| g.console)
            .filter(|&c| c != Console::Apps)
            .collect();
        cs.sort_by_key(|&c| c as usize);
        cs.dedup();
        cs.len()
    };
    let played: Vec<&Game> = here
        .iter()
        .copied()
        .filter(|g| stats.secs(&g.path) > 0)
        .collect();
    let total: u64 = played.iter().map(|g| stats.secs(&g.path)).sum();
    if total > 0 {
        now.push(stat("Played", span(total)));
    }
    let games_here = here.iter().filter(|g| g.console != Console::Apps).count();
    let library = format!(
        "{} on your cards for {}{}",
        count(games_here, "game"),
        count(consoles(&here), "system"),
        if total > 0 {
            format!(
                "; {} played, on {}.",
                played.len(),
                count(consoles(&played), "system")
            )
        } else {
            ", none played yet.".to_owned()
        }
    );
    let most = played.iter().max_by_key(|g| stats.secs(&g.path)).map(|g| {
        format!(
            "{} ({}), {}",
            g.name(),
            g.console.label(),
            span(stats.secs(&g.path))
        )
    });
    let board = board();
    About {
        name: board.map_or("This device", |b| &b.name).to_owned(),
        system: system().to_owned(),
        specs: board.map_or(&[], |b| &b.specs),
        source: board.map_or("", |b| &b.source),
        now,
        library,
        most,
    }
}

fn stat(label: &str, value: String) -> Stat {
    Stat {
        label: label.to_owned(),
        value,
        bar: None,
        share: None,
    }
}

fn count(n: usize, what: &str) -> String {
    match n {
        1 => format!("1 {what}"),
        n => format!("{n} {what}s"),
    }
}

/// A length of time, to the minute: "3 h 05 min", "12 min", "4 days 2 h".
pub fn span(secs: u64) -> String {
    let m = secs / 60;
    match m {
        0 => "under a minute".to_owned(),
        m if m < 60 => format!("{m} min"),
        m if m < 48 * 60 => format!("{} h {:02} min", m / 60, m % 60),
        m => format!("{} days {} h", m / (24 * 60), m / 60 % 24),
    }
}

/// A size in bytes, as storage is sold: "36 GB", "850 MB".
fn size(bytes: u64) -> String {
    const GB: f64 = 1e9;
    let b = bytes as f64;
    if b >= 100.0 * GB {
        format!("{:.0} GB", b / GB)
    } else if b >= GB {
        format!("{:.1} GB", b / GB).replace(".0 ", " ")
    } else {
        format!("{:.0} MB", b / 1e6)
    }
}

/// The system and when it was built: nibOS's release file where there is one, else its
/// hardware package's version and build date (from pacman's record of it), else the system's
/// own name.
fn system() -> &'static str {
    static SYSTEM: OnceLock<String> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        release(Path::new("/etc/nibos-release"))
            .or_else(|| package(Path::new("/var/lib/pacman/local"), "nibos-hw"))
            .or_else(|| os_release(Path::new("/etc/os-release")))
            .unwrap_or_else(|| "A desktop preview".to_owned())
    })
}

/// `KEY=value` lines, quotes taken off.
fn fields(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| {
        let (k, v) = l.split_once('=')?;
        Some((k.trim(), v.trim().trim_matches('"')))
    })
}

/// nibOS's release file: `VERSION=` and `BUILD_DATE=`.
fn release(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let get = |key| fields(&text).find(|&(k, _)| k == key).map(|(_, v)| v);
    let version = get("VERSION")?;
    Some(match get("BUILD_DATE") {
        Some(date) => format!("nibOS {version}, built {date}"),
        None => format!("nibOS {version}"),
    })
}

/// An installed package's version and build date, from pacman's database.
fn package(db: &Path, name: &str) -> Option<String> {
    let dir = std::fs::read_dir(db).ok()?.flatten().find(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        // `<name>-<pkgver>-<pkgrel>`: the name, then exactly two more parts.
        n.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|rest| rest.matches('-').count() == 1)
    })?;
    let desc = std::fs::read_to_string(dir.path().join("desc")).ok()?;
    let mut lines = desc.lines();
    let mut field = |key: &str| {
        lines
            .by_ref()
            .skip_while(|l| *l != key)
            .nth(1)
            .map(str::to_owned)
    };
    let version = field("%VERSION%")?;
    let built = field("%BUILDDATE%").and_then(|t| t.parse::<i64>().ok());
    Some(match built {
        Some(t) => format!("nibOS {version}, built {}", crate::stats::date(t)),
        None => format!("nibOS {version}"),
    })
}

/// The system's own name (`PRETTY_NAME`), on the stock OS.
fn os_release(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let name = fields(&text).find(|&(k, _)| k == "PRETTY_NAME")?.1;
    Some(name.to_owned())
}

/// What changes while the card is up, read every few seconds.
#[derive(Clone, Default)]
struct Readings {
    /// The charge and what it is doing (", charging").
    battery: Option<(u8, &'static str)>,
    protect: Option<bool>,
    /// Each card the games are on.
    space: Vec<Space>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Space {
    free: u64,
    total: u64,
}

fn readings() -> Readings {
    static LAST: Mutex<Option<(Instant, Readings)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, r)) = last.as_ref().filter(|(at, _)| at.elapsed() < EVERY) {
        return r.clone();
    }
    let battery = nibkit::battery::read().map(|b| {
        use nibkit::battery::State;
        let state = match b.state {
            State::Charging => ", charging",
            State::Full => ", full",
            State::NotCharging if b.plugged_in => ", on the charger",
            _ => "",
        };
        (b.percent, state)
    });
    let r = Readings {
        battery,
        protect: nibkit::battery::protect(),
        space: spaces(&crate::platform::rom_roots()),
    };
    *last = Some((Instant::now(), r.clone()));
    r
}

const EVERY: Duration = Duration::from_secs(5);

/// The room on each of the file systems `roots` are on, each once, in their order.
fn spaces(roots: &[PathBuf]) -> Vec<Space> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for root in roots {
        let Some((fsid, space)) = statvfs(root) else {
            continue;
        };
        if space.total > 0 && !seen.contains(&fsid) {
            seen.push(fsid);
            out.push(space);
        }
    }
    out
}

#[cfg(unix)]
fn statvfs(path: &Path) -> Option<(u64, Space)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs is plain integers, valid all zero.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: statvfs(3) reads the path and writes only the struct it is given.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let block = s.f_frsize as u64;
    #[allow(clippy::unnecessary_cast)]
    Some((
        s.f_fsid as u64,
        Space {
            free: s.f_bavail as u64 * block,
            total: s.f_blocks as u64 * block,
        },
    ))
}

#[cfg(not(unix))]
fn statvfs(_: &Path) -> Option<(u64, Space)> {
    None
}

/// How long the system has been up, sleep included.
#[cfg(target_os = "linux")]
fn uptime() -> Option<Duration> {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes the timespec it is given and nothing else.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut t) } == 0;
    ok.then(|| Duration::new(t.tv_sec as u64, t.tv_nsec as u32))
}

#[cfg(not(target_os = "linux"))]
fn uptime() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;

    #[test]
    fn every_board_reads() {
        for text in BOARDS {
            let b: Board = toml::from_str(text).unwrap();
            assert!(!b.name.is_empty() && !b.source.is_empty());
            assert!((4..=7).contains(&b.specs.len()), "{}", b.board);
        }
    }

    #[test]
    fn the_system_line() {
        let dir = TempDir::new("device-system");
        let db = dir.path().join("local");
        for (name, desc) in [
            (
                "nibos-hw-0.1.0-34",
                "%NAME%\nnibos-hw\n\n%VERSION%\n0.1.0-34\n\n%BUILDDATE%\n1792800000\n",
            ),
            (
                "nibos-hw-extra-1.0-1",
                "%NAME%\nnibos-hw-extra\n\n%VERSION%\n1.0-1\n",
            ),
        ] {
            fs::create_dir_all(db.join(name)).unwrap();
            fs::write(db.join(name).join("desc"), desc).unwrap();
        }
        let line = package(&db, "nibos-hw").unwrap();
        assert!(line.starts_with("nibOS 0.1.0-34, built "), "{line}");
        assert!(line.ends_with(" 2026"), "{line}");
        assert_eq!(package(&db, "nibos"), None);

        let rel = dir.path().join("nibos-release");
        fs::write(&rel, "NAME=nibOS\nVERSION=\"0.2\"\nBUILD_DATE=2026-10-21\n").unwrap();
        assert_eq!(
            release(&rel).as_deref(),
            Some("nibOS 0.2, built 2026-10-21")
        );
        let os = dir.path().join("os-release");
        fs::write(&os, "NAME=\"Linux\"\nPRETTY_NAME=\"Ubuntu 18.04\"\n").unwrap();
        assert_eq!(os_release(&os).as_deref(), Some("Ubuntu 18.04"));
    }

    #[test]
    fn spans_and_sizes() {
        assert_eq!(span(30), "under a minute");
        assert_eq!(span(12 * 60), "12 min");
        assert_eq!(span(3 * 3600 + 5 * 60), "3 h 05 min");
        assert_eq!(span(3 * 86400 + 7200), "3 days 2 h");
        assert_eq!(size(36_400_000_000), "36.4 GB");
        assert_eq!(size(2_000_000_000), "2 GB");
        assert_eq!(size(850_000_000), "850 MB");
        assert_eq!(size(128_000_000_000), "128 GB");
    }

    #[test]
    fn each_card_once() {
        let dir = TempDir::new("device-space");
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        // Two folders on one file system, and one that isn't there: one card.
        let found = spaces(&[a, b, dir.path().join("gone")]);
        assert_eq!(found.len(), 1);
        assert!(found[0].free <= found[0].total);
    }
}
