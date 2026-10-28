//! The shelf's achievements worker: games are asked about as they are shown, and looked up
//! off the shelf's thread, one at a time (the ROM's hash, then the set and the player's unlocks
//! from the cache or the service, set.rs), then their badges, read or fetched one by one. What
//! is found is kept by the game's path while the shelf is up, the sets shared rather than
//! copied ([`Answer::Set`] holds an `Arc`), for the shelf to draw from. Nothing here draws:
//! the badges come back as images, for the shelf to make into textures.
//!
//! Browsing quickly asks about many games in a moment. The worker looks up the latest asked
//! and tells the ones it passed over that they were skipped, so that they are asked again
//! when they are shown again, rather than left "Looking up…" for good.
//!
//! The worker is a thread of its own, not the shelf's scope's: leaving the shelf to start a
//! game doesn't wait for a request to the service. It stops when [`Lookups`] is dropped and it
//! next has something to tell. It reads the sign-in afresh for every lookup, and says when it
//! has changed since the last, so that what was found for the last player is let go.

use super::set::Answer;
use super::{Account, Ra};
use crate::gfx::{self, Img};
use crate::rom::Console;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

/// Where the worker gets what it looks up: the service and the cache ([`Ra`]), or a test's.
pub trait Source: Send + 'static {
    /// Who is signed in, now.
    fn account(&self) -> Option<Account>;
    /// What there is to show for a game, for `account`.
    fn look_up(&self, account: Option<&Account>, console: Console, path: &Path) -> Answer;
    /// A badge's picture, read or fetched.
    fn badge(&self, name: &str) -> Option<Img>;
}

impl Source for Ra {
    fn account(&self) -> Option<Account> {
        Ra::account(self)
    }

    fn look_up(&self, account: Option<&Account>, console: Console, path: &Path) -> Answer {
        self.store.look_up(account, console, path)
    }

    fn badge(&self, name: &str) -> Option<Img> {
        self.store.badge(name).and_then(|file| gfx::load_png(&file))
    }
}

/// A game asked about, in the round of asking it belongs to (`forget` starts a new one).
struct Ask {
    round: u64,
    console: Console,
    path: PathBuf,
}

/// What the worker tells.
enum Told {
    /// What a game asked about in `round` comes to.
    Answer(u64, PathBuf, Answer),
    /// A game asked about in `round` and passed over for one asked later.
    Skipped(u64, PathBuf),
    /// The sign-in has changed since the last lookup: what was found before was another's.
    Account,
    /// A badge, by name: its picture, or none when it couldn't be had.
    Badge(String, Option<Img>),
}

/// The games' achievements, as the worker finds them, by the game's path.
pub struct Lookups {
    ask: Sender<Ask>,
    told: Receiver<Told>,
    /// Bumped by `forget`: anything told about an earlier round is let go.
    round: u64,
    /// Asked about and not answered yet.
    pending: HashSet<PathBuf>,
    found: HashMap<PathBuf, Answer>,
    /// Badges of the sets found that haven't come in yet, and those that have.
    due: HashSet<String>,
    told_badges: HashSet<String>,
}

impl Lookups {
    /// Starts the worker on its own thread, looking up from `source`.
    pub fn start(source: impl Source) -> Lookups {
        let (ask, asked) = channel::<Ask>();
        let (tell, told) = channel();
        let spawned = std::thread::Builder::new()
            .name("dslot-cheevos".into())
            .spawn(move || work(&source, &asked, &tell));
        if let Err(e) = spawned {
            eprintln!("dslot: RetroAchievements: no worker: {e}");
        }
        Lookups {
            ask,
            told,
            round: 0,
            pending: HashSet::new(),
            found: HashMap::new(),
            due: HashSet::new(),
            told_badges: HashSet::new(),
        }
    }

    /// Asks about the game at `path` unless it has been answered or is being looked up. A
    /// console whose ROMs have no hash here (`achievements::hashed`) isn't asked about.
    pub fn ask(&mut self, console: Console, path: &Path) {
        if !super::hashed(console) || self.pending.contains(path) || self.found.contains_key(path) {
            return;
        }
        let ask = Ask {
            round: self.round,
            console,
            path: path.to_owned(),
        };
        if self.ask.send(ask).is_ok() {
            self.pending.insert(path.to_owned());
        }
    }

    /// What the game at `path` came to, once answered.
    pub fn answer(&self, path: &Path) -> Option<&Answer> {
        self.found.get(path)
    }

    /// Whether the game at `path` is being looked up.
    pub fn looking(&self, path: &Path) -> bool {
        self.pending.contains(path)
    }

    /// Whether anything is still to come in: a lookup, or a badge of a set found.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty() || !self.due.is_empty()
    }

    /// Takes in what the worker has told since: the answers are kept, the badges that came
    /// in are given back, by name, to be made into textures.
    pub fn poll(&mut self) -> Vec<(String, Img)> {
        let mut badges = Vec::new();
        while let Ok(told) = self.told.try_recv() {
            match told {
                Told::Answer(round, path, answer) if round == self.round => {
                    self.pending.remove(&path);
                    if let Answer::Set(set) = &answer {
                        let new = set.achievements.iter().map(|a| &a.badge);
                        let new = new.filter(|b| !self.told_badges.contains(*b));
                        self.due.extend(new.cloned());
                    }
                    self.found.insert(path, answer);
                }
                // Passed over while browsing: asked again when the game is shown again.
                Told::Skipped(round, path) if round == self.round => {
                    self.pending.remove(&path);
                }
                Told::Answer(..) | Told::Skipped(..) => {}
                Told::Account => self.found.clear(),
                Told::Badge(name, img) => {
                    self.due.remove(&name);
                    self.told_badges.insert(name.clone());
                    if let Some(img) = img {
                        badges.push((name, img));
                    }
                }
            }
        }
        badges
    }

    /// Forgets every game's answer, and anything still to come for what was asked before: the
    /// player signed in or out. Badges are anyone's and are kept.
    pub fn forget(&mut self) {
        self.round += 1;
        self.pending.clear();
        self.found.clear();
    }
}

/// The worker: the latest game asked about first, telling the ones passed over; then, with
/// nothing asked, the badges, those of the game looked up last first. It stops when no one is
/// listening.
fn work(source: &impl Source, asked: &Receiver<Ask>, tell: &Sender<Told>) {
    let mut sent: HashSet<String> = HashSet::new();
    let mut todo: VecDeque<String> = VecDeque::new();
    // Who the last lookup was for: None before the first.
    let mut signed: Option<Option<Account>> = None;
    loop {
        let first = if todo.is_empty() {
            match asked.recv() {
                Ok(first) => Some(first),
                Err(_) => return,
            }
        } else {
            match asked.try_recv() {
                Ok(first) => Some(first),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        };
        if let Some(first) = first {
            let mut all: Vec<Ask> = std::iter::once(first).chain(asked.try_iter()).collect();
            let latest = all.pop().expect("one at least");
            for Ask { round, path, .. } in all {
                if tell.send(Told::Skipped(round, path)).is_err() {
                    return;
                }
            }
            let account = source.account();
            if signed.as_ref().is_some_and(|was| *was != account)
                && tell.send(Told::Account).is_err()
            {
                return;
            }
            let answer = source.look_up(account.as_ref(), latest.console, &latest.path);
            signed = Some(account);
            if let Answer::Set(set) = &answer {
                // The badges this game needs, soonest first; the others after.
                let wanted: Vec<_> = set
                    .achievements
                    .iter()
                    .map(|a| a.badge.clone())
                    .filter(|b| !sent.contains(b))
                    .collect();
                todo.retain(|b| !wanted.contains(b));
                for b in wanted.into_iter().rev() {
                    todo.push_front(b);
                }
            }
            if tell
                .send(Told::Answer(latest.round, latest.path, answer))
                .is_err()
            {
                return;
            }
            continue;
        }
        let Some(name) = todo.pop_front() else {
            continue;
        };
        if !sent.insert(name.clone()) {
            continue;
        }
        let img = source.badge(&name);
        if tell.send(Told::Badge(name, img)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::set::{Achievement, Set};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// A service that says which game it is looking up, and answers only when let.
    struct Fake {
        account: Arc<Mutex<Option<Account>>>,
        started: Mutex<Sender<PathBuf>>,
        gate: Mutex<Receiver<()>>,
    }

    impl Source for Fake {
        fn account(&self) -> Option<Account> {
            self.account.lock().unwrap().clone()
        }

        fn look_up(&self, account: Option<&Account>, _: Console, path: &Path) -> Answer {
            let _ = self.started.lock().unwrap().send(path.to_owned());
            let _ = self.gate.lock().unwrap().recv();
            match account {
                None => Answer::Nothing,
                Some(a) => Answer::Set(Arc::new(Set {
                    id: 1,
                    title: a.user.clone(),
                    achievements: vec![Achievement {
                        id: 1,
                        title: "First".into(),
                        description: String::new(),
                        points: 5,
                        badge: "00001".into(),
                        unlocked: false,
                        unlocked_at: None,
                    }],
                })),
            }
        }

        fn badge(&self, _: &str) -> Option<Img> {
            Some(Img {
                w: 1,
                h: 1,
                px: vec![0; 4],
            })
        }
    }

    struct World {
        lookups: Lookups,
        account: Arc<Mutex<Option<Account>>>,
        started: Receiver<PathBuf>,
        gate: Sender<()>,
    }

    fn world(signed_in: bool) -> World {
        let account = Arc::new(Mutex::new(signed_in.then(|| alan("t"))));
        let (started_tx, started) = channel();
        let (gate, gate_rx) = channel();
        let lookups = Lookups::start(Fake {
            account: account.clone(),
            started: Mutex::new(started_tx),
            gate: Mutex::new(gate_rx),
        });
        World {
            lookups,
            account,
            started,
            gate,
        }
    }

    fn alan(token: &str) -> Account {
        Account {
            user: "Alan".into(),
            token: token.into(),
        }
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/r/GBA/{name}.gba"))
    }

    impl World {
        /// Waits for the worker to start looking up `name`.
        fn started(&self, name: &str) {
            let got = self.started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(got, path(name));
        }

        /// Polls until nothing is to come in.
        fn settle(&mut self) -> Vec<String> {
            let until = Instant::now() + Duration::from_secs(5);
            let mut badges = Vec::new();
            loop {
                badges.extend(self.lookups.poll().into_iter().map(|(n, _)| n));
                if !self.lookups.waiting() {
                    return badges;
                }
                assert!(Instant::now() < until, "still waiting");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn games_passed_over_while_browsing_are_asked_again_not_left_looking() {
        let mut w = world(true);
        w.lookups.ask(Console::Gba, &path("X"));
        w.started("X");
        // Browsing on past three while X is looked up.
        for name in ["A", "B", "C"] {
            w.lookups.ask(Console::Gba, &path(name));
        }
        w.gate.send(()).unwrap();
        w.started("C");
        w.gate.send(()).unwrap();
        assert_eq!(w.settle(), ["00001"]);
        assert!(matches!(w.lookups.answer(&path("X")), Some(Answer::Set(_))));
        assert!(w.lookups.answer(&path("C")).is_some());
        // A and B were skipped: neither looking nor answered, so asked again when shown.
        for name in ["A", "B"] {
            assert!(!w.lookups.looking(&path(name)));
            assert_eq!(w.lookups.answer(&path(name)), None);
        }
        w.lookups.ask(Console::Gba, &path("A"));
        assert!(w.lookups.looking(&path("A")));
        w.started("A");
        w.gate.send(()).unwrap();
        w.settle();
        assert!(w.lookups.answer(&path("A")).is_some());
        // Answered: not asked again.
        w.lookups.ask(Console::Gba, &path("A"));
        assert!(!w.lookups.looking(&path("A")));
        // Nor a console whose games have no hash here.
        w.lookups
            .ask(Console::GameWatch, &PathBuf::from("/r/GW/gnw_ball.zip"));
        assert!(!w.lookups.waiting());
    }

    #[test]
    fn forgetting_lets_go_of_answers_still_to_come() {
        let mut w = world(true);
        w.lookups.ask(Console::Gba, &path("X"));
        w.started("X");
        // Signed out meanwhile: the shelf forgets; X's answer, for the last player, is let go.
        *w.account.lock().unwrap() = None;
        w.lookups.forget();
        w.gate.send(()).unwrap();
        w.lookups.ask(Console::Gba, &path("X"));
        w.started("X");
        w.gate.send(()).unwrap();
        w.settle();
        assert_eq!(w.lookups.answer(&path("X")), Some(&Answer::Nothing));
    }

    #[test]
    fn a_new_sign_in_found_by_the_worker_lets_go_of_the_last_players_answers() {
        let mut w = world(false);
        w.lookups.ask(Console::Gba, &path("X"));
        w.started("X");
        w.gate.send(()).unwrap();
        w.settle();
        assert_eq!(w.lookups.answer(&path("X")), Some(&Answer::Nothing));
        // Signed in where the shelf didn't hear of it (the menu closed mid-sign-in): the next
        // lookup notices, and X is asked again when shown.
        *w.account.lock().unwrap() = Some(alan("t"));
        w.lookups.ask(Console::Gba, &path("Y"));
        w.started("Y");
        w.gate.send(()).unwrap();
        w.settle();
        assert!(matches!(w.lookups.answer(&path("Y")), Some(Answer::Set(_))));
        assert_eq!(w.lookups.answer(&path("X")), None);
    }
}
