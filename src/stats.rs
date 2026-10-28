//! Play time, favourites and ratings per game, kept in a small tab-separated file
//! (`stats.tsv`, platform::dirs).

use crate::gfx::Mark;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default, Clone, Copy)]
struct Stat {
    secs: u64,
    last: i64,
    sessions: u32,
    fave: bool,
    /// Stars out of five; 0 is unrated.
    rating: u8,
}

pub struct Stats {
    file: PathBuf,
    map: HashMap<String, Stat>,
}

impl Stats {
    pub fn load(file: PathBuf) -> Stats {
        let mut map = HashMap::new();
        for line in fs::read_to_string(&file).unwrap_or_default().lines() {
            let f: Vec<&str> = line.split('\t').collect();
            // The last two columns were added later, so older files have four.
            if let [path, secs, last, sessions, rest @ ..] = &f[..] {
                let stat = Stat {
                    secs: secs.parse().unwrap_or(0),
                    last: last.parse().unwrap_or(0),
                    sessions: sessions.parse().unwrap_or(0),
                    fave: rest.first() == Some(&"1"),
                    rating: rest.get(1).and_then(|r| r.parse().ok()).unwrap_or(0),
                };
                map.insert((*path).to_owned(), stat);
            }
        }
        Stats { file, map }
    }

    pub fn record(&mut self, rom: &Path, secs: u64) {
        let s = self
            .map
            .entry(rom.to_string_lossy().into_owned())
            .or_default();
        s.secs += secs;
        s.last = now();
        s.sessions += 1;
        self.save();
    }

    pub fn toggle_fave(&mut self, rom: &Path) {
        let s = self
            .map
            .entry(rom.to_string_lossy().into_owned())
            .or_default();
        s.fave = !s.fave;
        self.save();
    }

    pub fn set_rating(&mut self, rom: &Path, rating: u8) {
        self.map
            .entry(rom.to_string_lossy().into_owned())
            .or_default()
            .rating = rating;
        self.save();
    }

    fn save(&self) {
        let line = |(p, s): (&String, &Stat)| {
            format!(
                "{p}\t{}\t{}\t{}\t{}\t{}\n",
                s.secs, s.last, s.sessions, s.fave as u8, s.rating
            )
        };
        let out: String = self.map.iter().map(line).collect();
        let written = self
            .file
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&self.file, out));
        if let Err(e) = written {
            eprintln!("dslot: cannot write {}: {e}", self.file.display());
        }
    }

    fn get(&self, rom: &Path) -> Stat {
        self.map
            .get(rom.to_string_lossy().as_ref())
            .copied()
            .unwrap_or_default()
    }

    pub fn fave(&self, rom: &Path) -> bool {
        self.get(rom).fave
    }

    pub fn rating(&self, rom: &Path) -> u8 {
        self.get(rom).rating
    }

    /// When the game was last played, in seconds since the epoch; 0 for never.
    pub fn last(&self, rom: &Path) -> i64 {
        self.get(rom).last
    }

    /// How long a game has been played, in seconds.
    pub fn secs(&self, rom: &Path) -> u64 {
        self.get(rom).secs
    }

    pub fn played(&self, rom: &Path) -> String {
        match self.get(rom).secs / 60 {
            0 if self.get(rom).secs == 0 => "Not yet".into(),
            0 => "Under 1 min".into(),
            m if m < 60 => format!("{m} min"),
            m => format!("{} h {:02} min", m / 60, m % 60),
        }
    }

    /// How many times a game has been played.
    pub fn session_count(&self, rom: &Path) -> u32 {
        self.get(rom).sessions
    }

    /// The longest any game has been played, in seconds, and the most times: what a game's
    /// own is measured against on its card.
    pub fn most(&self) -> (u64, u32) {
        self.map.values().fold((0, 0), |(secs, sessions), s| {
            (secs.max(s.secs), sessions.max(s.sessions))
        })
    }

    pub fn sessions(&self, rom: &Path) -> String {
        self.get(rom).sessions.to_string()
    }

    pub fn last_played(&self, rom: &Path) -> String {
        let last = self.get(rom).last;
        if last == 0 {
            return "Never".into();
        }
        let (then, today) = (local(last), local(now()));
        let days = (today.tm_year - then.tm_year) * 366 + today.tm_yday - then.tm_yday;
        match days {
            0 => "Today".into(),
            1 => "Yesterday".into(),
            _ => date(last),
        }
    }
}

/// When something was saved, for a label: "today, 14:32", "yesterday, 9:05 pm", or the date.
pub fn saved_at(t: i64) -> String {
    let (then, today) = (local(t), local(now()));
    let days = (today.tm_year - then.tm_year) * 366 + today.tm_yday - then.tm_yday;
    let time = || {
        nibkit::clock::format(
            then.tm_hour as u32,
            then.tm_min as u32,
            nibkit::clock::hour12(),
        )
    };
    match days {
        0 => format!("today, {}", time()),
        1 => format!("yesterday, {}", time()),
        _ => date(t),
    }
}

/// A time as a local date: "21 Oct 2026".
pub fn date(t: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let tm = local(t);
    format!(
        "{} {} {}",
        tm.tm_mday,
        MONTHS[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_year + 1900
    )
}

fn now() -> i64 {
    // SAFETY: time(2) with no pointer only returns the time.
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

fn local(t: i64) -> libc::tm {
    let t = t as libc::time_t;
    // SAFETY: struct tm is plain integers (and a pointer localtime_r sets), valid all zero.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r only writes the struct it is given.
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

/// The time for the status bar, 12-hour or 24-hour as nibOS's preference says (read again
/// every second, so a change made elsewhere shows at once).
pub fn clock() -> String {
    static HOUR12: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut cached = HOUR12.lock().unwrap_or_else(|e| e.into_inner());
    let hour12 = match *cached {
        Some((at, h12)) if at.elapsed() < Duration::from_secs(1) => h12,
        _ => {
            let h12 = nibkit::clock::hour12();
            *cached = Some((Instant::now(), h12));
            h12
        }
    };
    let tm = local(now());
    nibkit::clock::format(tm.tm_hour as u32, tm.tm_min as u32, hour12)
}

/// The battery for the status bar: its charge, and a bolt or a plug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryNow {
    pub percent: u8,
    pub mark: Mark,
}

/// The battery now, for the status bar: nibOS's own count (`nibkit::battery`), else the first
/// battery sysfs has. Read again every few seconds, not every frame.
pub fn battery() -> Option<BatteryNow> {
    static LAST: Mutex<Option<(Instant, Option<BatteryNow>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, now)) = last.as_ref().filter(|(at, _)| at.elapsed() < BATTERY_EVERY) {
        return *now;
    }
    let now = nibkit::battery::read().map(|b| BatteryNow {
        percent: b.percent,
        mark: mark(b.state, b.plugged_in),
    });
    let now = now.or_else(|| {
        let rd = fs::read_dir("/sys/class/power_supply").ok()?;
        rd.flatten().find_map(|e| {
            let percent = fs::read_to_string(e.path().join("capacity")).ok()?;
            let status = fs::read_to_string(e.path().join("status")).unwrap_or_default();
            Some(BatteryNow {
                percent: percent.trim().parse().ok()?,
                mark: if status.trim() == "Charging" {
                    Mark::Charging
                } else {
                    Mark::None
                },
            })
        })
    });
    *last = Some((Instant::now(), now));
    now
}

/// The mark beside the battery: a bolt while it charges, a plug on the charger otherwise.
fn mark(state: nibkit::battery::State, plugged_in: bool) -> Mark {
    use nibkit::battery::State;
    match state {
        State::Charging => Mark::Charging,
        State::Full => Mark::Plugged,
        State::NotCharging if plugged_in => Mark::Plugged,
        _ => Mark::None,
    }
}

const BATTERY_EVERY: Duration = Duration::from_secs(5);

/// The battery as the System page shows it: its charge and what it is doing.
pub fn battery_line(b: &nibkit::battery::Battery, protect: bool) -> String {
    use nibkit::battery::State;
    let percent = b.percent;
    match b.state {
        State::Charging => match b.time_to_full() {
            Some(t) => {
                let mins = t.as_secs().div_ceil(60).max(1);
                let to = if protect { "80%" } else { "full" };
                format!("{percent}% \u{b7} {to} in ~{mins} min")
            }
            None => format!("{percent}% \u{b7} charging"),
        },
        State::NotCharging if b.plugged_in && protect => {
            format!("{percent}% \u{b7} holding (protect)")
        }
        State::NotCharging if b.plugged_in => format!("{percent}% \u{b7} not charging"),
        State::Full => format!("{percent}% \u{b7} full"),
        _ => format!("{percent}%"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn battery_lines() {
        use nibkit::battery::{Battery, State};
        let b = |percent, state, plugged_in, current_ua| Battery {
            percent,
            state,
            plugged_in,
            charge_uah: 2_000_000,
            full_uah: 4_000_000,
            current_ua,
        };
        assert_eq!(
            battery_line(&b(62, State::Discharging, false, -300_000), false),
            "62%"
        );
        assert_eq!(
            battery_line(&b(50, State::Charging, true, 1_000_000), false),
            "50% \u{b7} full in ~120 min"
        );
        assert_eq!(
            battery_line(&b(50, State::Charging, true, 0), true),
            "50% \u{b7} charging"
        );
        assert_eq!(
            battery_line(&b(80, State::NotCharging, true, 0), true),
            "80% \u{b7} holding (protect)"
        );
        assert_eq!(
            battery_line(&b(100, State::Full, true, 0), false),
            "100% \u{b7} full"
        );
    }

    #[test]
    fn round_trip() {
        let dir = TempDir::new("stats");
        let file = dir.path().join("share/dslot/stats.tsv");
        let rom = Path::new("/mnt/roms/Roms/NDS/Lantern Tide.nds");
        let mut stats = Stats::load(file.clone());
        assert_eq!(
            (
                stats.played(rom),
                stats.last_played(rom),
                stats.sessions(rom)
            ),
            ("Not yet".into(), "Never".into(), "0".into())
        );

        stats.record(rom, 30);
        stats.record(rom, 3700);
        stats.toggle_fave(rom);
        stats.set_rating(rom, 4);
        let stats = Stats::load(file.clone());
        assert_eq!(stats.played(rom), "1 h 02 min");
        assert_eq!(stats.sessions(rom), "2");
        assert_eq!(stats.last_played(rom), "Today");
        assert!(stats.fave(rom) && stats.rating(rom) == 4);

        // A file from before favourites and ratings, with four columns.
        fs::write(&file, "/r/a.nds\t45\t0\t1\n").unwrap();
        let old = Stats::load(file);
        assert_eq!(old.played(Path::new("/r/a.nds")), "Under 1 min");
        assert!(!old.fave(Path::new("/r/a.nds")));
    }
}
