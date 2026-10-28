//! A game's achievements, for the shelf: what the service publishes for it and which of them
//! the player has, looked up by the ROM's hash (hash.rs) and kept on disk, so that the shelf
//! shows them without the network and doesn't ask the service each time a game is selected.
//!
//! Two requests of the emulator API, with the sign-in kept: `patch` (the set, by the hash:
//! each achievement's title, description, points and badge name) and `unlocks` (the ids the
//! player has, softcore then hardcore). The badges are small PNGs from the service's media
//! host, kept in a folder DSperate shares, by name.

use super::{call, fetch, hash, urlencode, Account, Fail};
use crate::rom::Console;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The media host the badges come from.
pub const MEDIA: &str = "https://media.retroachievements.org";
/// How long a lookup stands before the service is asked again; a game just played is asked
/// about again at once (`played`).
const FRESH: Duration = Duration::from_secs(6 * 60 * 60);
/// `Flags` of an achievement in the set proper; others are unofficial, not shown.
const CORE: u64 = 3;

/// One achievement, as the shelf shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Achievement {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub points: u32,
    /// The badge's name on the media host: `<name>.png` there, and in the badges folder.
    pub badge: String,
    #[serde(default)]
    pub unlocked: bool,
}

/// A game's set, and the player's standing in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Set {
    pub id: u32,
    pub title: String,
    pub achievements: Vec<Achievement>,
}

impl Set {
    pub fn unlocked(&self) -> usize {
        self.achievements.iter().filter(|a| a.unlocked).count()
    }

    /// Points earned, and points in all.
    pub fn points(&self) -> (u32, u32) {
        self.achievements.iter().fold((0, 0), |(got, all), a| {
            (got + if a.unlocked { a.points } else { 0 }, all + a.points)
        })
    }

    /// "12 of 40 · 150 of 400 points"
    pub fn summary(&self) -> String {
        let (got, all) = self.points();
        format!(
            "{} of {} \u{b7} {got} of {all} points",
            self.unlocked(),
            self.achievements.len()
        )
    }
}

/// What a lookup found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// The service doesn't know this ROM.
    Unknown,
    Set(Set),
}

/// What is kept for a hash: when it was fetched, and what.
#[derive(Serialize, Deserialize)]
struct Cached {
    fetched: u64,
    set: Option<Set>,
}

/// Where lookups keep what they found.
#[derive(Clone, Debug)]
pub struct Store {
    /// DSlot's cache: `achievements/<hash>.json` and the hashes index.
    pub cache: PathBuf,
    /// The badges, shared with the emulators: `~/.cache/retroachievements/badges/<name>.png`.
    pub badges: PathBuf,
}

impl Store {
    /// Under DSlot's cache folder, with the badges where every app looks for them.
    pub fn new(cache: &Path) -> Store {
        let home = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
        Store {
            cache: cache.join("achievements"),
            badges: match home {
                Some(h) => h.join("retroachievements/badges"),
                None => cache.join("badges"),
            },
        }
    }

    fn file(&self, hash: &str) -> PathBuf {
        self.cache.join(format!("{hash}.json"))
    }

    /// The badge's file, fetched when it isn't there yet. None when it can't be had.
    pub fn badge(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\', '.']) {
            return None;
        }
        let file = self.badges.join(format!("{name}.png"));
        if file.is_file() {
            return Some(file);
        }
        match fetch(&format!("{MEDIA}/Badge/{name}.png"), &file) {
            Ok(()) => Some(file),
            Err(_) => None,
        }
    }

    /// The hash of a ROM, from the index when the file hasn't changed since, else worked out
    /// and remembered: a DS ROM's takes a read of a few megabytes.
    pub fn hash(&self, console: Console, path: &Path) -> Option<String> {
        let index = self.cache.join("hashes.tsv");
        let meta = std::fs::metadata(path).ok()?;
        let stamp = format!(
            "{}\t{}",
            meta.len(),
            meta.modified()
                .ok()
                .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs())
        );
        let key = path.to_string_lossy();
        let known = std::fs::read_to_string(&index).unwrap_or_default();
        for line in known.lines() {
            let mut f = line.rsplitn(2, '\t');
            let (hash, rest) = (f.next()?, f.next()?);
            if let Some(p) = rest.strip_suffix(&format!("\t{stamp}")) {
                if p == key {
                    return Some(hash.to_owned());
                }
            }
        }
        let hash = hash::of(console, path)?;
        let _ = std::fs::create_dir_all(&self.cache);
        let mut lines: Vec<&str> = known
            .lines()
            .filter(|l| !l.starts_with(&format!("{key}\t")))
            .collect();
        let line = format!("{key}\t{stamp}\t{hash}");
        lines.push(&line);
        let _ = std::fs::write(&index, lines.join("\n") + "\n");
        Some(hash)
    }

    /// What is kept for the hash, however old.
    fn cached(&self, hash: &str) -> Option<Cached> {
        let text = std::fs::read_to_string(self.file(hash)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn keep(&self, hash: &str, cached: &Cached) {
        let _ = std::fs::create_dir_all(&self.cache);
        if let Ok(text) = serde_json::to_string(cached) {
            let _ = std::fs::write(self.file(hash), text);
        }
    }

    /// The game was just played: what is kept for it is asked about again next time.
    pub fn played(&self, console: Console, path: &Path) {
        let Some(hash) = self.hash(console, path) else {
            return;
        };
        if let Some(mut cached) = self.cached(&hash) {
            cached.fetched = 0;
            self.keep(&hash, &cached);
        }
    }

    /// A game's achievements: from the cache while it's fresh, else from the service, which
    /// needs the sign-in; with no network, whatever is kept. None when nothing is known.
    pub fn look_up(
        &self,
        account: Option<&Account>,
        console: Console,
        path: &Path,
    ) -> Option<Found> {
        let hash = self.hash(console, path)?;
        let kept = self.cached(&hash);
        let now = unix_now();
        if let Some(c) = &kept {
            if c.fetched + FRESH.as_secs() > now {
                return Some(found(&c.set));
            }
        }
        let Some(account) = account else {
            return kept.map(|c| found(&c.set));
        };
        match fetch_set(account, &hash) {
            Ok(set) => {
                let cached = Cached { fetched: now, set };
                self.keep(&hash, &cached);
                Some(found(&cached.set))
            }
            Err(why) => {
                if let Fail::Failed(w) = &why {
                    eprintln!("dslot: RetroAchievements: {w}");
                }
                kept.map(|c| found(&c.set))
            }
        }
    }
}

fn found(set: &Option<Set>) -> Found {
    match set {
        Some(s) => Found::Set(s.clone()),
        None => Found::Unknown,
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The set for a hash and the player's unlocks in it, from the service. Ok(None): the service
/// doesn't know the ROM.
fn fetch_set(account: &Account, hash: &str) -> Result<Option<Set>, Fail> {
    let auth = format!(
        "u={}&t={}",
        urlencode(&account.user),
        urlencode(&account.token)
    );
    let body = call(&format!("r=patch&{auth}&m={}", urlencode(hash)))?;
    let Some(mut set) = parse_patch(&body)? else {
        return Ok(None);
    };
    let mut unlocked = Vec::new();
    for hardcore in [0, 1] {
        let body = call(&format!("r=unlocks&{auth}&g={}&h={hardcore}", set.id))?;
        unlocked.extend(parse_unlocks(&body)?);
    }
    for a in &mut set.achievements {
        a.unlocked = unlocked.contains(&a.id);
    }
    Ok(Some(set))
}

/// The `patch` answer: the set, or None when the service says it doesn't know the game.
pub fn parse_patch(body: &str) -> Result<Option<Set>, Fail> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| Fail::Failed("the service answered with something other than JSON".into()))?;
    if json.get("Success").and_then(|v| v.as_bool()) != Some(true) {
        let why = json
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("the service said no");
        // An unknown hash is answered with an error naming it, not a set: that's an answer.
        let why_l = why.to_ascii_lowercase();
        if why_l.contains("unknown game")
            || why_l.contains("not found")
            || why_l.contains("no game")
        {
            return Ok(None);
        }
        return Err(Fail::Failed(why.to_owned()));
    }
    let Some(data) = json.get("PatchData") else {
        return Err(Fail::Failed("the service's answer had no set in it".into()));
    };
    let id = data.get("ID").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if id == 0 {
        return Ok(None);
    }
    let title = data
        .get("Title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let text = |a: &serde_json::Value, key: &str| {
        a.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let achievements = data
        .get("Achievements")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter(|a| a.get("Flags").and_then(|v| v.as_u64()) == Some(CORE))
                .map(|a| Achievement {
                    id: a.get("ID").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    title: text(a, "Title"),
                    description: text(a, "Description"),
                    points: a.get("Points").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    badge: text(a, "BadgeName"),
                    unlocked: false,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Set {
        id,
        title,
        achievements,
    }))
}

/// The `unlocks` answer: the ids the player has.
pub fn parse_unlocks(body: &str) -> Result<Vec<u32>, Fail> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| Fail::Failed("the service answered with something other than JSON".into()))?;
    if json.get("Success").and_then(|v| v.as_bool()) != Some(true) {
        let why = json
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("the service said no");
        return Err(Fail::Failed(why.to_owned()));
    }
    Ok(json
        .get("UserUnlocks")
        .and_then(|v| v.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|v| v.as_u64())
                .map(|v| v as u32)
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = r#"{"Success":true,"PatchData":{"ID":12345,"Title":"Advance Wars: Days of Ruin","ConsoleID":18,"ImageIcon":"/Images/000001.png","Achievements":[
      {"ID":1,"Title":"First Blood","Description":"Win a battle","Points":5,"Flags":3,"BadgeName":"00001","MemAddr":"0x1=1"},
      {"ID":2,"Title":"Test only","Description":"Unofficial","Points":1,"Flags":5,"BadgeName":"00002","MemAddr":"0x1=1"},
      {"ID":3,"Title":"Veteran","Description":"Win ten","Points":25,"Flags":3,"BadgeName":"00003","MemAddr":"0x1=1"}
    ],"Leaderboards":[]}}"#;

    #[test]
    fn a_set_with_its_core_achievements() {
        let set = parse_patch(PATCH).unwrap().unwrap();
        assert_eq!(
            (set.id, set.title.as_str()),
            (12345, "Advance Wars: Days of Ruin")
        );
        assert_eq!(set.achievements.len(), 2);
        assert_eq!(set.achievements[1].badge, "00003");
        assert_eq!(set.points(), (0, 30));
        let ids = parse_unlocks(r#"{"Success":true,"UserUnlocks":[3,99],"GameID":12345}"#).unwrap();
        assert_eq!(ids, [3, 99]);
        let mut set = set;
        for a in &mut set.achievements {
            a.unlocked = ids.contains(&a.id);
        }
        assert_eq!(set.summary(), "1 of 2 \u{b7} 25 of 30 points");
    }

    #[test]
    fn unknown_and_refused() {
        assert_eq!(
            parse_patch(r#"{"Success":false,"Error":"Unknown game"}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_patch(r#"{"Success":false,"Error":"Credentials invalid (0)"}"#),
            Err(Fail::Failed("Credentials invalid (0)".into()))
        );
        assert!(parse_unlocks("not json").is_err());
    }

    #[test]
    fn kept_on_disk_and_asked_again_after_a_game() {
        let dir = crate::testing::TempDir::new("ra-set");
        let store = Store {
            cache: dir.path().join("cache"),
            badges: dir.path().join("badges"),
        };
        let rom = dir.path().join("Game.gba");
        std::fs::write(&rom, b"a gba game").unwrap();
        let hash = store.hash(Console::Gba, &rom).unwrap();
        // Remembered: the same again without reading the ROM.
        std::fs::write(&rom, b"a gba game").unwrap();
        assert_eq!(store.hash(Console::Gba, &rom).unwrap(), hash);
        assert!(store.cache.join("hashes.tsv").is_file());
        // Nothing kept, no sign-in: nothing known.
        assert_eq!(store.look_up(None, Console::Gba, &rom), None);
        let set = parse_patch(PATCH).unwrap();
        store.keep(
            &hash,
            &Cached {
                fetched: unix_now(),
                set: set.clone(),
            },
        );
        assert_eq!(
            store.look_up(None, Console::Gba, &rom),
            Some(Found::Set(set.clone().unwrap()))
        );
        // Played: what is kept is stale, but still what there is without a sign-in.
        store.played(Console::Gba, &rom);
        assert_eq!(store.cached(&hash).unwrap().fetched, 0);
        assert_eq!(
            store.look_up(None, Console::Gba, &rom),
            Some(Found::Set(set.unwrap()))
        );
        store.keep(
            &hash,
            &Cached {
                fetched: unix_now(),
                set: None,
            },
        );
        assert_eq!(
            store.look_up(None, Console::Gba, &rom),
            Some(Found::Unknown)
        );
        assert_eq!(store.badge("../etc"), None);
    }
}
