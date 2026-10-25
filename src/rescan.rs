//! Noticing games copied onto a card or deleted from it while DSlot runs, and cards put in or
//! taken out. Each console's folders are watched (inotify, on Linux), and a console whose
//! folders changed is looked at again once they have been quiet for a second: a copy still
//! being written keeps them busy. The looking is done on a thread of its own; the shelf takes
//! in what was found between frames (`library::merge`). Menu → System → Rescan games looks at
//! every console at once, for a change the watch can't see.

use crate::rom::{self, Console, Game};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// How long a console's folders must be quiet before they are looked at again.
const QUIET: Duration = Duration::from_secs(1);
/// The longest the watcher waits for a change before it sees whether it has been asked for
/// anything.
const TICK: Duration = Duration::from_millis(200);

/// Where each console's games are: its folders, by `Console as usize` (none for a console with
/// no page), and the folder its known-games file is in.
#[derive(Clone)]
pub struct Folders {
    pub searched: [Vec<PathBuf>; Console::COUNT],
    pub cache: PathBuf,
}

/// What the watcher found.
pub enum Found {
    /// Every game now in a console's folders.
    Games(Console, Vec<Game>),
    /// The rescan asked for with `Rescan::now` is done: every console has been looked at.
    Done,
}

enum Ask {
    /// Watch these folders from now on: the consoles with pages changed.
    Watch(Box<Folders>),
    /// Look at every console now.
    Now,
}

/// The watcher, which runs until DSlot exits.
pub struct Rescan {
    asks: Sender<Ask>,
    found: Receiver<Found>,
}

impl Rescan {
    pub fn start(folders: Folders) -> Rescan {
        let (asks, asked) = channel();
        let (tell, found) = channel();
        let spawned = std::thread::Builder::new()
            .name("rescan".into())
            .spawn(move || watch(folders, asked, tell));
        if let Err(e) = spawned {
            eprintln!("dslot: no watch on the games' folders: {e}");
        }
        Rescan { asks, found }
    }

    /// Watches `folders` from now on.
    pub fn watch(&self, folders: Folders) {
        let _ = self.asks.send(Ask::Watch(Box::new(folders)));
    }

    /// Looks at every console now; `Found::Done` says when it has.
    pub fn now(&self) {
        let _ = self.asks.send(Ask::Now);
    }

    /// What has been found since the last call.
    pub fn found(&self) -> Vec<Found> {
        self.found.try_iter().collect()
    }
}

fn watch(mut folders: Folders, asked: Receiver<Ask>, tell: Sender<Found>) {
    let mut notify = Notify::new();
    let mut present = notify.set(&folders);
    // The consoles waiting to be looked at, and when their folders last changed.
    let mut due: [Option<Instant>; Console::COUNT] = [None; Console::COUNT];
    loop {
        let now = Instant::now();
        let wait = due
            .iter()
            .flatten()
            .map(|&t| (t + QUIET).saturating_duration_since(now))
            .min()
            .unwrap_or(TICK)
            .min(TICK);
        let changed = notify.wait(wait);
        let mut all_now = false;
        loop {
            match asked.try_recv() {
                Ok(Ask::Watch(f)) => {
                    folders = *f;
                    present = notify.set(&folders);
                }
                Ok(Ask::Now) => all_now = true,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let now = Instant::now();
        match changed {
            Changed::Consoles(consoles) => {
                for c in consoles {
                    due[c as usize] = Some(now);
                }
            }
            // A card in or out, a console's folder made or gone: watch again, look at all.
            Changed::All => {
                present = notify.set(&folders);
                for c in Console::ALL {
                    due[c as usize] = Some(now);
                }
            }
            Changed::Nothing => {}
        }
        if all_now {
            present = notify.set(&folders);
        }
        for console in Console::ALL {
            let quiet = due[console as usize].is_some_and(|t| now >= t + QUIET);
            if !(all_now || quiet) {
                continue;
            }
            due[console as usize] = None;
            let dirs = &present[console as usize];
            if folders.searched[console as usize].is_empty() {
                continue;
            }
            let games = rom::scan(console, dirs, &folders.cache.join(console.known()));
            crate::mark(&format!("{} games rescanned", console.label()));
            if tell.send(Found::Games(console, games)).is_err() {
                return;
            }
        }
        if all_now && tell.send(Found::Done).is_err() {
            return;
        }
    }
}

/// What changed while the watcher waited.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum Changed {
    Nothing,
    /// Something in these consoles' folders.
    Consoles(Vec<Console>),
    /// A card, or a folder the consoles' folders are in.
    All,
}

/// The kernel's notice of changes to folders (inotify).
#[cfg(target_os = "linux")]
struct Notify {
    fd: libc::c_int,
    watches: std::collections::HashMap<libc::c_int, On>,
}

/// What a watch is on.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum On {
    /// One of a console's folders.
    Console(Console),
    /// A Roms folder, which the consoles' folders are in.
    Roms,
    /// udev's list of the cards' partitions.
    Cards,
}

/// Where udev lists the cards' partitions: a card put in or taken out changes them.
#[cfg(target_os = "linux")]
const CARDS: &[&str] = &["/dev/disk/by-label", "/dev/disk/by-uuid"];

#[cfg(target_os = "linux")]
impl Notify {
    fn new() -> Notify {
        Notify {
            fd: -1,
            watches: Default::default(),
        }
    }

    /// Watches `folders` (and only them) from now on. Returns each console's folders that are
    /// there now, on a card that is in: a Roms folder that isn't (on nibOS, a card slot's
    /// automount with no card, which takes seconds to say so) is asked about once, here.
    fn set(&mut self, folders: &Folders) -> [Vec<PathBuf>; Console::COUNT] {
        use libc::*;
        if self.fd >= 0 {
            // SAFETY: a descriptor this owns; closing it drops every watch on it.
            unsafe { close(self.fd) };
        }
        // SAFETY: no pointers; the result is checked.
        self.fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
        self.watches.clear();
        let mut add = |path: &std::path::Path, mask: u32, what: On| {
            let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
                return false;
            };
            // SAFETY: a NUL-terminated path that outlives the call.
            let wd = unsafe { inotify_add_watch(self.fd, c.as_ptr(), mask | IN_ONLYDIR) };
            if wd >= 0 {
                self.watches.insert(wd, what);
            }
            wd >= 0
        };
        let gone = IN_DELETE_SELF | IN_MOVE_SELF;
        let mut roots: Vec<(PathBuf, bool)> = Vec::new();
        let mut present: [Vec<PathBuf>; Console::COUNT] = Default::default();
        for console in Console::ALL {
            for dir in &folders.searched[console as usize] {
                let Some(root) = dir.parent() else { continue };
                let there = match roots.iter().find(|(r, _)| r == root) {
                    Some(&(_, there)) => there,
                    None => {
                        // Folders made or removed in a Roms folder: a console's own, maybe.
                        let mask = IN_CREATE | IN_DELETE | IN_MOVED_FROM | IN_MOVED_TO | gone;
                        let there = add(root, mask, On::Roms);
                        roots.push((root.to_owned(), there));
                        there
                    }
                };
                if !there {
                    continue;
                }
                let mask = IN_CREATE
                    | IN_DELETE
                    | IN_MOVED_FROM
                    | IN_MOVED_TO
                    | IN_MODIFY
                    | IN_CLOSE_WRITE
                    | gone;
                add(dir, mask, On::Console(console));
                present[console as usize].push(dir.clone());
            }
        }
        for cards in CARDS {
            let mask = IN_CREATE | IN_DELETE | IN_MOVED_TO | IN_MOVED_FROM;
            add(std::path::Path::new(cards), mask, On::Cards);
        }
        present
    }

    /// Waits up to `limit` for changes, and says what they were to.
    fn wait(&mut self, limit: Duration) -> Changed {
        use libc::*;
        if self.fd < 0 {
            std::thread::sleep(limit);
            return Changed::Nothing;
        }
        let mut poll_fd = pollfd {
            fd: self.fd,
            events: POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd, which outlives the call.
        let ready = unsafe { poll(&mut poll_fd, 1, limit.as_millis() as c_int) };
        if ready <= 0 {
            return Changed::Nothing;
        }
        let mut consoles = Vec::new();
        let mut all = false;
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: reads into `buf`, no further than its length.
            let n = unsafe { read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            let mut at = 0;
            let head = std::mem::size_of::<inotify_event>();
            while at + head <= n as usize {
                // SAFETY: the kernel wrote a whole event header here; read unaligned.
                let event: inotify_event =
                    unsafe { std::ptr::read_unaligned(buf[at..].as_ptr().cast()) };
                at += head + event.len as usize;
                let lost = event.mask & (IN_IGNORED | IN_UNMOUNT | IN_DELETE_SELF | IN_MOVE_SELF);
                match self.watches.get(&event.wd) {
                    _ if event.mask & IN_Q_OVERFLOW != 0 || lost != 0 => all = true,
                    Some(On::Console(console)) => consoles.push(*console),
                    // Only folders matter in a Roms folder.
                    Some(On::Roms) => all |= event.mask & IN_ISDIR != 0,
                    Some(On::Cards) => all = true,
                    None => {}
                }
            }
        }
        if all {
            Changed::All
        } else if consoles.is_empty() {
            Changed::Nothing
        } else {
            consoles.dedup();
            Changed::Consoles(consoles)
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Notify {
    fn drop(&mut self) {
        if self.fd >= 0 {
            // SAFETY: a descriptor this owns, closed once.
            unsafe { libc::close(self.fd) };
        }
    }
}

/// Elsewhere (a desktop, trying the shelf out) nothing is watched: Rescan games still works.
#[cfg(not(target_os = "linux"))]
struct Notify;

#[cfg(not(target_os = "linux"))]
impl Notify {
    fn new() -> Notify {
        Notify
    }

    fn set(&mut self, folders: &Folders) -> [Vec<PathBuf>; Console::COUNT] {
        folders.searched.clone()
    }

    fn wait(&mut self, wait: Duration) -> Changed {
        std::thread::sleep(wait);
        Changed::Nothing
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;

    /// Waits a few seconds for what the watcher finds next that `want` takes.
    fn next<T>(rescan: &Rescan, want: impl Fn(Found) -> Option<T>) -> T {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if let Some(t) = rescan.found().into_iter().find_map(&want) {
                return t;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the watcher found nothing");
    }

    #[test]
    fn notices_a_game_copied_in_and_deleted() {
        let dir = TempDir::new("rescan");
        let gba = dir.path().join("Roms/GBA");
        fs::create_dir_all(&gba).unwrap();
        let mut searched: [Vec<PathBuf>; Console::COUNT] = Default::default();
        searched[Console::Gba as usize] = vec![gba.clone()];
        let rescan = Rescan::start(Folders {
            searched,
            cache: dir.path().to_owned(),
        });
        // Its watches are set as it starts.
        std::thread::sleep(Duration::from_millis(300));
        let games = |f: Found| match f {
            Found::Games(Console::Gba, games) => Some(games.len()),
            _ => None,
        };
        fs::write(gba.join("Moth & Lamp (Europe).gba"), [0u8; 512]).unwrap();
        assert_eq!(next(&rescan, games), 1);
        fs::remove_file(gba.join("Moth & Lamp (Europe).gba")).unwrap();
        assert_eq!(next(&rescan, games), 0);
        // Asked for, every console is looked at at once, and it says when it has.
        rescan.now();
        next(&rescan, |f| matches!(f, Found::Done).then_some(()));
    }
}
