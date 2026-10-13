//! Play time, favourites and ratings per game, kept in a small tab-separated file next to the binary.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
        let s = self.map.entry(rom.to_string_lossy().into_owned()).or_default();
        s.secs += secs;
        s.last = now();
        s.sessions += 1;
        self.save();
    }

    pub fn toggle_fave(&mut self, rom: &Path) {
        let s = self.map.entry(rom.to_string_lossy().into_owned()).or_default();
        s.fave = !s.fave;
        self.save();
    }

    pub fn set_rating(&mut self, rom: &Path, rating: u8) {
        self.map.entry(rom.to_string_lossy().into_owned()).or_default().rating = rating;
        self.save();
    }

    fn save(&self) {
        let line = |(p, s): (&String, &Stat)| format!("{p}\t{}\t{}\t{}\t{}\t{}\n", s.secs, s.last, s.sessions, s.fave as u8, s.rating);
        let out: String = self.map.iter().map(line).collect();
        if let Err(e) = fs::write(&self.file, out) {
            eprintln!("stats: cannot write {}: {e}", self.file.display());
        }
    }

    fn get(&self, rom: &Path) -> Stat {
        self.map.get(rom.to_string_lossy().as_ref()).copied().unwrap_or_default()
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

    pub fn played(&self, rom: &Path) -> String {
        match self.get(rom).secs / 60 {
            0 => "Not yet".into(),
            m if m < 60 => format!("{m} min"),
            m => format!("{} h {:02} min", m / 60, m % 60),
        }
    }

    pub fn sessions(&self, rom: &Path) -> String {
        self.get(rom).sessions.to_string()
    }

    pub fn last_played(&self, rom: &Path) -> String {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let last = self.get(rom).last;
        if last == 0 {
            return "Never".into();
        }
        let (then, today) = (local(last), local(now()));
        let days = (today.tm_year - then.tm_year) * 366 + today.tm_yday - then.tm_yday;
        match days {
            0 => "Today".into(),
            1 => "Yesterday".into(),
            _ => format!("{} {} {}", then.tm_mday, MONTHS[then.tm_mon.clamp(0, 11) as usize], then.tm_year + 1900),
        }
    }
}

fn now() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

fn local(t: i64) -> libc::tm {
    let t = t as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

pub fn clock() -> String {
    let tm = local(now());
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// Battery percentage from sysfs, when the platform exposes one.
pub fn battery() -> Option<String> {
    let rd = fs::read_dir("/sys/class/power_supply").ok()?;
    rd.flatten().find_map(|e| {
        let cap = fs::read_to_string(e.path().join("capacity")).ok()?;
        Some(format!("{}%", cap.trim()))
    })
}
