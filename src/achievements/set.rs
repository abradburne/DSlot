//! A game's achievements, for the shelf: what the service publishes for it and which of them
//! the player has, looked up by the ROM's hash (nib-cheevos' `hash`, rcheevos' rhash) and kept
//! on disk, so that the shelf
//! shows them without the network and doesn't ask the service each time a game is selected.
//!
//! What is kept is the signed-in player's: `achievements/<user>/<hash>.json`, so that another
//! account's unlocks are never shown, nor taken for what changed since. The hashes index
//! (`achievements/hashes.tsv`, a ROM's path, size and date against its hash) is anyone's. The
//! badges are small PNGs from the service's media host, kept in a folder DSperate shares, by
//! name.

use super::service::{self, Error, WARNING_ID};
use super::Account;
use crate::rom::Console;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// How long a lookup stands before the service is asked again; a game just played is asked
/// about again at once (`played`).
const FRESH: Duration = Duration::from_secs(6 * 60 * 60);
/// The hashes index, in the cache folder.
const INDEX: &str = "hashes.tsv";

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
    /// When it was earned, as far as DSlot knows (Unix seconds): the lookup that first found it
    /// earned, when an earlier one had found it not. None for one already earned the first
    /// time the game was looked up, or not earned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unlocked_at: Option<u64>,
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

    /// "12 of 40 · 150 of 400 points", "1,000 of 1,250 points" grouped as the standing is.
    pub fn summary(&self) -> String {
        let (got, all) = self.points();
        format!(
            "{} of {} \u{b7} {} of {} points",
            super::grouped(self.unlocked() as u64),
            super::grouped(self.achievements.len() as u64),
            super::grouped(got.into()),
            super::grouped(all.into()),
        )
    }
}

/// An achievement the player has earned, and the game it is in: for the Achievements page's
/// list of them all ([`Store::earned`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Earned {
    pub game: String,
    pub achievement: Achievement,
}

/// "42 earned · 380 points · in 7 games", or "Nothing earned yet".
pub fn earned_summary(earned: &[Earned]) -> String {
    if earned.is_empty() {
        return "Nothing earned yet".into();
    }
    let points: u64 = earned.iter().map(|e| u64::from(e.achievement.points)).sum();
    let games = earned
        .iter()
        .map(|e| e.game.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    format!(
        "{} earned \u{b7} {} points \u{b7} in {} game{}",
        super::grouped(earned.len() as u64),
        super::grouped(points),
        super::grouped(games as u64),
        if games == 1 { "" } else { "s" },
    )
}

/// What a lookup comes to, for the shelf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Nothing to show: signed out, a console the service's hash isn't reproduced for, a ROM
    /// that can't be read, or no network with nothing kept.
    Nothing,
    /// The service doesn't know this ROM.
    Unknown,
    /// The set, and the player's standing in it. Shared, not copied, by whoever shows it.
    Set(Arc<Set>),
    /// The service refused the sign-in kept (a token revoked by a password change, say), and
    /// nothing fresh is kept: the player is to sign in again.
    Refused,
}

/// What is kept for a hash: when it was fetched, and what.
#[derive(Serialize, Deserialize)]
struct Cached {
    fetched: u64,
    set: Option<Set>,
}

impl Cached {
    fn answer(&self) -> Answer {
        match &self.set {
            Some(s) => Answer::Set(Arc::new(s.clone())),
            None => Answer::Unknown,
        }
    }
}

/// Where lookups keep what they found.
#[derive(Clone, Debug)]
pub struct Store {
    /// DSlot's cache: `achievements/<user>/<hash>.json` and the hashes index.
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

    /// The player's own folder: their name as the service spells it, anything but letters,
    /// digits, `-` and `_` made `_`, so that no name reaches outside the cache.
    fn folder(&self, user: &str) -> PathBuf {
        let name: String = user
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '_',
            })
            .collect();
        self.cache.join(if name.is_empty() { "_" } else { &name })
    }

    fn file(&self, user: &str, hash: &str) -> PathBuf {
        self.folder(user).join(format!("{hash}.json"))
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
        service::badge(name, &file).ok().map(|()| file)
    }

    /// The hash the index has for the ROM as it is now (its size and date unchanged), without
    /// reading the ROM: for the main thread, which mustn't wait on a read of a whole game.
    pub fn indexed(&self, path: &Path) -> Option<String> {
        let (key, stamp) = index_key(path)?;
        let known = std::fs::read_to_string(self.cache.join(INDEX)).ok()?;
        find(&known, &key, &stamp)
    }

    /// The hash of a ROM, from the index when the file hasn't changed since, else worked out
    /// and remembered: a DS ROM's takes a read of a few megabytes, a GBA ROM's all of it.
    pub fn hash(&self, console: Console, path: &Path) -> Option<String> {
        let index = self.cache.join(INDEX);
        let Some((key, stamp)) = index_key(path) else {
            // A path the index can't hold (a tab or a newline in it): worked out every time.
            return hash_of(console, path);
        };
        let known = std::fs::read_to_string(&index).unwrap_or_default();
        if let Some(hash) = find(&known, &key, &stamp) {
            return Some(hash);
        }
        let hash = hash_of(console, path)?;
        let mut lines: Vec<&str> = known
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with(&format!("{key}\t")))
            .collect();
        let line = format!("{key}\t{stamp}\t{hash}");
        lines.push(&line);
        if let Err(e) = write_whole(&index, &(lines.join("\n") + "\n")) {
            eprintln!("dslot: cannot write {}: {e}", index.display());
        }
        Some(hash)
    }

    /// What is kept for the player and the hash, however old.
    fn cached(&self, user: &str, hash: &str) -> Option<Cached> {
        let text = std::fs::read_to_string(self.file(user, hash)).ok()?;
        let mut cached: Cached = serde_json::from_str(&text).ok()?;
        // Kept before the warning entries were left out.
        if let Some(set) = &mut cached.set {
            set.achievements.retain(|a| a.id < WARNING_ID);
        }
        Some(cached)
    }

    fn keep(&self, user: &str, hash: &str, cached: &Cached) {
        if let Ok(text) = serde_json::to_string(cached) {
            let _ = write_whole(&self.file(user, hash), &text);
        }
    }

    /// Every achievement the player has earned in the games kept for them, however old what is
    /// kept: those dated first, the latest at the top, then the rest by game, in set order.
    /// Each once, though two ROMs of a game keep its set twice. Only what lookups have kept:
    /// a game never shown on the shelf here isn't in it.
    pub fn earned(&self, user: &str) -> Vec<Earned> {
        let Ok(dir) = std::fs::read_dir(self.folder(user)) else {
            return Vec::new();
        };
        let mut sets: Vec<Set> = dir
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let hash = name.strip_suffix(".json")?;
                self.cached(user, hash)?.set
            })
            .collect();
        // The same order whatever order the folder lists them in.
        sets.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
        let mut seen = std::collections::HashSet::new();
        let mut earned: Vec<Earned> = sets
            .into_iter()
            .flat_map(|set| {
                let game = set.title;
                set.achievements
                    .into_iter()
                    .filter(|a| a.unlocked)
                    .map(move |a| Earned {
                        game: game.clone(),
                        achievement: a,
                    })
            })
            .filter(|e| seen.insert(e.achievement.id))
            .collect();
        // Stable: undated ones keep the game order.
        earned.sort_by_key(|e| std::cmp::Reverse(e.achievement.unlocked_at));
        earned
    }

    /// The game was just played: what is kept of it for the player is asked about again next
    /// time. From the index alone: a ROM never looked up has nothing kept to make stale.
    pub fn played(&self, user: &str, path: &Path) {
        let Some(hash) = self.indexed(path) else {
            return;
        };
        if let Some(mut cached) = self.cached(user, &hash) {
            cached.fetched = 0;
            self.keep(user, &hash, &cached);
        }
    }

    /// A game's achievements for the player signed in: from the cache while it's fresh, else
    /// from the service; with no network, or the service failing, whatever is kept. Signed
    /// out, nothing: what is kept is the last player's, not for anyone.
    pub fn look_up(&self, account: Option<&Account>, console: Console, path: &Path) -> Answer {
        self.look_up_by(account, console, path, service::set)
    }

    /// `look_up`, with the request that fetches a set given (the service's, or a test's).
    fn look_up_by(
        &self,
        account: Option<&Account>,
        console: Console,
        path: &Path,
        fetch: impl FnOnce(&Account, &str) -> Result<Option<Set>, Error>,
    ) -> Answer {
        let Some(account) = account else {
            return Answer::Nothing;
        };
        let Some(hash) = self.hash(console, path) else {
            return Answer::Nothing;
        };
        let user = account.user.as_str();
        let kept = self.cached(user, &hash);
        let now = unix_now();
        if let Some(c) = &kept {
            if c.fetched + FRESH.as_secs() > now {
                return c.answer();
            }
        }
        match fetch(account, &hash) {
            Ok(set) => {
                let before = kept.and_then(|c| c.set);
                let set = set.map(|s| dated(s, before.as_ref(), now));
                let cached = Cached { fetched: now, set };
                self.keep(user, &hash, &cached);
                cached.answer()
            }
            Err(Error::Refused(why)) => {
                eprintln!("dslot: RetroAchievements: the sign-in kept was refused: {why}");
                Answer::Refused
            }
            Err(why) => {
                if let Error::Failed(w) = &why {
                    eprintln!("dslot: RetroAchievements: {w}");
                }
                kept.map_or(Answer::Nothing, |c| c.answer())
            }
        }
    }
}

/// The ROM's hash as the service spells it (32 lowercase hex digits), by rcheevos' rule for its
/// console: none when the file can't be read, isn't shaped like the console's game, or is of a
/// kind not hashed yet (a CHD image), which is logged.
fn hash_of(console: Console, path: &Path) -> Option<String> {
    nib_cheevos::hash::file(console.folder(), path)
        .inspect_err(|e| {
            eprintln!(
                "dslot: RetroAchievements: no hash for {}: {e}",
                path.display()
            )
        })
        .ok()
}

/// What the index files a ROM under: its path, and its size and date as `size\tseconds`.
/// None for a file that isn't there, or a path with a tab or a newline in it, which would
/// break the index's lines.
fn index_key(path: &Path) -> Option<(String, String)> {
    let meta = std::fs::metadata(path).ok()?;
    let key = path.to_string_lossy();
    if key.contains(['\t', '\n', '\r']) {
        return None;
    }
    let stamp = format!(
        "{}\t{}",
        meta.len(),
        meta.modified()
            .ok()
            .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    );
    Some((key.into_owned(), stamp))
}

/// The hash on the index's line for `key` with `stamp`. A line not shaped as the index's
/// (cut short by a crash, say) is passed over, not the end of the index.
fn find(known: &str, key: &str, stamp: &str) -> Option<String> {
    let tail = format!("\t{stamp}");
    known.lines().find_map(|line| {
        let (rest, hash) = line.rsplit_once('\t')?;
        (rest.strip_suffix(&tail)? == key && !hash.is_empty()).then(|| hash.to_owned())
    })
}

/// Writes `text` to `to` whole or not at all: to a file beside it first, then renamed over it,
/// so that a crash or a second writer never leaves half a file where it is read.
fn write_whole(to: &Path, text: &str) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = to.file_name().unwrap_or_default().to_string_lossy();
    let part = to.with_file_name(format!(".{name}.{}.{n}.part", std::process::id()));
    std::fs::write(&part, text)
        .and_then(|()| std::fs::rename(&part, to))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&part);
        })
}

/// `set` as just fetched, with when each achievement was earned: kept from `before` (what the
/// last lookup found), or `now` for one earned since. With nothing before to go by, unknown.
fn dated(mut set: Set, before: Option<&Set>, now: u64) -> Set {
    for a in &mut set.achievements {
        if !a.unlocked {
            continue;
        }
        let was = before.and_then(|b| b.achievements.iter().find(|o| o.id == a.id));
        a.unlocked_at = match was {
            Some(o) if o.unlocked => o.unlocked_at,
            Some(_) => Some(now),
            // Not in the set before (added to it since), or no set before: when is unknown.
            None => None,
        };
    }
    set
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(unlocked: &[u32]) -> Set {
        let one = |id: u32, points| Achievement {
            id,
            title: format!("Achievement {id}"),
            description: String::new(),
            points,
            badge: format!("{id:05}"),
            unlocked: unlocked.contains(&id),
            unlocked_at: None,
        };
        Set {
            id: 12345,
            title: "Advance Wars: Days of Ruin".into(),
            achievements: vec![one(1, 5), one(3, 25)],
        }
    }

    fn account(user: &str) -> Account {
        Account {
            user: user.into(),
            token: format!("{user}-token"),
        }
    }

    fn store(dir: &crate::testing::TempDir) -> Store {
        Store {
            cache: dir.path().join("cache"),
            badges: dir.path().join("badges"),
        }
    }

    #[test]
    fn the_summary_counts_and_groups() {
        assert_eq!(set(&[3]).summary(), "1 of 2 \u{b7} 25 of 30 points");
        let mut big = set(&[1, 3]);
        big.achievements[0].points = 1000;
        assert_eq!(big.summary(), "2 of 2 \u{b7} 1,025 of 1,025 points");
    }

    #[test]
    fn earned_since_the_last_lookup_is_dated() {
        // The first lookup: earned already, but when isn't known.
        let first = dated(set(&[1]), None, 100);
        assert_eq!(first.achievements[0].unlocked_at, None);
        // 3 earned since: dated by the lookup that found it; 1 stays unknown.
        let second = dated(set(&[1, 3]), Some(&first), 200);
        let at: Vec<_> = second.achievements.iter().map(|a| a.unlocked_at).collect();
        assert_eq!(at, [None, Some(200)]);
        // Asked again later: the date is kept, not moved on.
        let third = dated(set(&[1, 3]), Some(&second), 300);
        assert_eq!(third.achievements[1].unlocked_at, Some(200));
        // Reset on the site: no longer earned, no date.
        let reset = dated(set(&[1]), Some(&third), 400);
        assert_eq!(reset.achievements[1].unlocked_at, None);
        // Kept on disk as it was; a cache from before the dates still reads.
        let text = serde_json::to_string(&third).unwrap();
        assert_eq!(serde_json::from_str::<Set>(&text).unwrap(), third);
        assert!(!serde_json::to_string(&first)
            .unwrap()
            .contains("unlocked_at"));
    }

    #[test]
    fn kept_on_disk_and_asked_again_after_a_game() {
        let dir = crate::testing::TempDir::new("ra-set");
        let store = store(&dir);
        let rom = dir.path().join("Game.gba");
        std::fs::write(&rom, b"a gba game").unwrap();
        let alan = account("Alan");
        let asked = std::cell::Cell::new(0);
        let service = |found: Option<Set>| {
            let asked = &asked;
            move |_: &Account, _: &str| {
                asked.set(asked.get() + 1);
                Ok(found)
            }
        };
        let got = store.look_up_by(Some(&alan), Console::Gba, &rom, service(Some(set(&[3]))));
        assert_eq!(got, Answer::Set(Arc::new(set(&[3]))));
        let hash = store.indexed(&rom).unwrap();
        assert!(store.file("Alan", &hash).is_file());
        // Fresh: the service isn't asked again.
        let again = store.look_up_by(Some(&alan), Console::Gba, &rom, service(None));
        assert_eq!((again, asked.get()), (Answer::Set(Arc::new(set(&[3]))), 1));
        // Played: asked again, and what it says now is kept, with 1 dated.
        store.played("Alan", &rom);
        assert_eq!(store.cached("Alan", &hash).unwrap().fetched, 0);
        let Answer::Set(now) =
            store.look_up_by(Some(&alan), Console::Gba, &rom, service(Some(set(&[1, 3]))))
        else {
            panic!("a set");
        };
        assert_eq!(asked.get(), 2);
        assert!(now.achievements[0].unlocked_at.is_some());
        // Stale and no network: what is kept. Refused: sign in again.
        store.played("Alan", &rom);
        let offline = store.look_up_by(Some(&alan), Console::Gba, &rom, |_, _| {
            Err(Error::NoNetwork)
        });
        assert_eq!(offline, Answer::Set(now.clone()));
        let refused = store.look_up_by(Some(&alan), Console::Gba, &rom, |_, _| {
            Err(Error::Refused("Expired".into()))
        });
        assert_eq!(refused, Answer::Refused);
        // The service not knowing it is kept too.
        store.played("Alan", &rom);
        let unknown = store.look_up_by(Some(&alan), Console::Gba, &rom, service(None));
        assert_eq!(unknown, Answer::Unknown);
        assert_eq!(
            store.look_up_by(Some(&alan), Console::Gba, &rom, service(Some(set(&[])))),
            Answer::Unknown
        );
        assert_eq!(store.badge("../etc"), None);
    }

    #[test]
    fn another_players_unlocks_are_never_shown_and_signed_out_nothing_is() {
        let dir = crate::testing::TempDir::new("ra-set-users");
        let store = store(&dir);
        let rom = dir.path().join("Game.gba");
        std::fs::write(&rom, b"a gba game").unwrap();
        let ok = |s: Set| move |_: &Account, _: &str| Ok(Some(s));
        let alan = store.look_up_by(Some(&account("Alan")), Console::Gba, &rom, ok(set(&[1, 3])));
        assert_eq!(alan, Answer::Set(Arc::new(set(&[1, 3]))));
        // Bea's own lookup, though Alan's is fresh: hers asks the service, and dates nothing
        // by Alan's.
        let bea = store.look_up_by(Some(&account("Bea")), Console::Gba, &rom, ok(set(&[1])));
        assert_eq!(bea, Answer::Set(Arc::new(set(&[1]))));
        // Signed out: nothing, not the last player's.
        assert_eq!(
            store.look_up_by(None, Console::Gba, &rom, ok(set(&[1]))),
            Answer::Nothing
        );
        // A name is never a way out of the cache.
        assert_eq!(store.folder("../x"), store.cache.join("___x"));
        assert_eq!(store.folder(""), store.cache.join("_"));
    }

    /// Everything earned in the sets kept for the player: the dated latest first, then by
    /// game; each once though a game is kept for two ROMs; another player's never.
    #[test]
    fn everything_earned_is_listed_once_the_latest_first() {
        let dir = crate::testing::TempDir::new("ra-set-earned");
        let store = store(&dir);
        assert_eq!(store.earned("Alan"), []);
        assert_eq!(earned_summary(&[]), "Nothing earned yet");
        let keep = |user: &str, hash: &str, set: Option<Set>| {
            store.keep(user, hash, &Cached { fetched: 0, set });
        };
        let mut wars = set(&[1, 3]);
        wars.achievements[1].unlocked_at = Some(200);
        let mut sun = set(&[1]);
        (sun.id, sun.title) = (999, "Golden Sun".into());
        sun.achievements[0].id = 7;
        keep("Alan", "aaa", Some(wars.clone()));
        keep("Alan", "bbb", Some(wars));
        keep("Alan", "ccc", Some(sun));
        keep("Alan", "ddd", None);
        keep("Bea", "eee", Some(set(&[1, 3])));
        std::fs::write(store.folder("Alan").join("notes.txt"), "not a set").unwrap();
        let earned = store.earned("Alan");
        let got: Vec<_> = earned
            .iter()
            .map(|e| (e.game.as_str(), e.achievement.id))
            .collect();
        assert_eq!(
            got,
            [
                ("Advance Wars: Days of Ruin", 3),
                ("Advance Wars: Days of Ruin", 1),
                ("Golden Sun", 7),
            ]
        );
        assert_eq!(
            earned_summary(&earned),
            "3 earned \u{b7} 35 points \u{b7} in 2 games"
        );
        assert_eq!(
            earned_summary(&earned[2..]),
            "1 earned \u{b7} 5 points \u{b7} in 1 game"
        );
    }

    #[test]
    fn a_bad_line_in_the_index_is_passed_over() {
        let dir = crate::testing::TempDir::new("ra-set-index");
        let store = store(&dir);
        let rom = dir.path().join("Game.gba");
        std::fs::write(&rom, b"a gba game").unwrap();
        std::fs::create_dir_all(&store.cache).unwrap();
        // A line cut short, and an empty one, before anything this ROM's.
        std::fs::write(store.cache.join(INDEX), "/r/GBA/cut short\n\n").unwrap();
        assert_eq!(store.indexed(&rom), None);
        let hash = store.hash(Console::Gba, &rom).unwrap();
        assert_eq!(store.indexed(&rom), Some(hash.clone()));
        let index = std::fs::read_to_string(store.cache.join(INDEX)).unwrap();
        assert!(index.starts_with("/r/GBA/cut short\n"), "{index}");
        assert_eq!(index.lines().count(), 2);
        // Another ROM is still remembered after it.
        let other = dir.path().join("Other.gba");
        std::fs::write(&other, b"another").unwrap();
        let second = store.hash(Console::Gba, &other).unwrap();
        assert_eq!(store.indexed(&other), Some(second));
        assert_eq!(store.indexed(&rom), Some(hash));
        // Changed since: no longer what the index says, until hashed again.
        std::fs::write(&rom, b"a longer gba game").unwrap();
        assert_eq!(store.indexed(&rom), None);
        // Nothing written beside it is left behind.
        let left: Vec<_> = std::fs::read_dir(&store.cache)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(left.is_empty());
    }

    #[test]
    fn a_set_kept_with_the_warning_entry_loads_without_it() {
        let dir = crate::testing::TempDir::new("ra-set-warning");
        let store = store(&dir);
        let mut warned = set(&[]);
        warned.achievements.push(Achievement {
            id: WARNING_ID,
            title: "Warning: Unknown Emulator".into(),
            description: String::new(),
            points: 0,
            badge: "00000".into(),
            unlocked: true,
            unlocked_at: None,
        });
        let cached = Cached {
            fetched: unix_now(),
            set: Some(warned),
        };
        store.keep("Alan", "abc", &cached);
        assert_eq!(store.cached("Alan", "abc").unwrap().set, Some(set(&[])));
    }
}
