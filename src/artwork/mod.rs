//! On-device box art, cart label, and disc art downloader for DSlot.
//!
//! Combines Libretro Thumbnails, GameTDB (DS covers & labels), `art.slot-cfw.fyi` (DS/GBA/GB/GBC
//! WebP sticker scans by CRC32), No-Intro / FBNeo DATs, and LaunchBox Games Database indexes
//! (`data/art/<console>.tsv.z` -> `images.launchbox-app.com`) for every cart and disc console.
//!
//! Runs on a small pool of background worker threads so the shelf stays responsive, can be
//! cancelled safely at any point, and refreshes each game's cover/cart live as soon as its file
//! lands in `Imgs/`, `Labels/`, or `Carts/`.

pub mod checksum;
pub mod fetch;
pub mod launchbox;
pub mod matching;

pub use fetch::{eligible, needs_art, ArtKind, Service};

use crate::rom::{Console, Game};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

const WORKERS: usize = 3;

/// Which set of games an artwork download job targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Game(PathBuf),
    Console(Console),
    All,
}

/// Request from the menu to start or stop an artwork job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Start {
        scope: Scope,
        kind: ArtKind,
        replace: bool,
    },
    Stop,
}

/// Live status of the active or most recent artwork job, shown in the menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub running: bool,
    pub label: String,
    pub kind: ArtKind,
    pub total: usize,
    pub done: usize,
    pub saved_box: usize,
    pub saved_cart: usize,
    pub missing: usize,
    pub errors: usize,
    pub current: String,
    pub stopped: bool,
    pub last_error: Option<String>,
}

impl Snapshot {
    /// Short one-line summary for a menu row value.
    pub fn summary(&self) -> String {
        if self.running {
            let found = self.saved_box + self.saved_cart;
            return format!("{}/{} ({found} saved)", self.done, self.total);
        }
        if self.total == 0 {
            return "All art present".into();
        }
        let mut parts = Vec::new();
        if self.saved_box > 0 {
            parts.push(format!("{} box", self.saved_box));
        }
        if self.saved_cart > 0 {
            parts.push(format!("{} cart", self.saved_cart));
        }
        let head = if parts.is_empty() {
            if self.errors > 0 {
                self.last_error
                    .clone()
                    .unwrap_or_else(|| "No network".into())
            } else if self.stopped {
                "Stopped".into()
            } else {
                "Nothing found".into()
            }
        } else {
            let prefix = if self.stopped {
                "Stopped · "
            } else {
                "Saved "
            };
            format!("{prefix}{}", parts.join(", "))
        };
        if self.missing > 0 && !parts.is_empty() {
            format!("{head} · {} not found", self.missing)
        } else {
            head
        }
    }

    /// Compact label and optional progress fraction (`0.0..=1.0`) for the shelf's corner
    /// progress pill while a background job is running or right after it finishes.
    pub fn pill(&self) -> Option<(String, Option<f32>)> {
        if self.total == 0 {
            return None;
        }
        if self.running {
            let saved = self.saved_box + self.saved_cart;
            let text = if saved > 0 {
                format!(
                    "Artwork  {}/{}  \u{b7}  {} saved",
                    self.done, self.total, saved
                )
            } else {
                format!("Artwork  {}/{}", self.done, self.total)
            };
            let frac = (self.done as f32 / self.total.max(1) as f32).clamp(0.0, 1.0);
            Some((text, Some(frac)))
        } else {
            Some((format!("Artwork  \u{b7}  {}", self.summary()), None))
        }
    }

    /// Lightweight fingerprint of fields visible in the progress pill and menu summary, used to
    /// wake a resting shelf only when artwork progress actually advances.
    pub fn progress_key(&self) -> (bool, usize, usize, usize, usize, usize, bool) {
        (
            self.running,
            self.total,
            self.done,
            self.saved_box,
            self.saved_cart,
            self.errors,
            self.stopped,
        )
    }
}

/// Counts how many games in `games` are missing artwork of `kind` (optionally filtered to one console).
pub fn missing_count(games: &[Game], console: Option<Console>, kind: ArtKind) -> usize {
    games
        .iter()
        .filter(|g| console.is_none_or(|c| g.console == c))
        .filter(|g| needs_art(g, kind))
        .count()
}

/// Background artwork batch runner owned by the shelf so jobs keep running if the menu closes.
pub struct Runner {
    service: Arc<Service>,
    cancel: Arc<AtomicBool>,
    batch: Arc<AtomicU64>,
    state: Arc<Mutex<Snapshot>>,
    queue: Option<Arc<Mutex<VecDeque<Game>>>>,
    active_kind: ArtKind,
    updated: Option<Receiver<PathBuf>>,
}

impl Runner {
    pub fn new(cache: &Path, art_index_dirs: Vec<PathBuf>) -> Runner {
        Runner {
            service: Arc::new(Service::new(cache, art_index_dirs)),
            cancel: Arc::new(AtomicBool::new(false)),
            batch: Arc::new(AtomicU64::new(0)),
            state: Arc::new(Mutex::new(Snapshot::default())),
            queue: None,
            active_kind: ArtKind::Both,
            updated: None,
        }
    }

    pub fn available(&self) -> bool {
        fetch::curl_available()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }

    /// Stops any running job cleanly; already-saved files stay on disk.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let mut st = self.state.lock().unwrap();
        if st.running {
            st.stopped = true;
        }
    }

    /// Starts a new batch for `targets` (cancelling any previous run first).
    pub fn start(&mut self, label: String, targets: Vec<Game>, kind: ArtKind, replace: bool) {
        self.cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();

        let total = targets.len();
        if total == 0 {
            *self.state.lock().unwrap() = Snapshot {
                running: false,
                label,
                kind,
                total: 0,
                ..Default::default()
            };
            self.updated = None;
            return;
        }

        let state = Arc::new(Mutex::new(Snapshot {
            running: true,
            label,
            kind,
            total,
            ..Default::default()
        }));
        self.state = state.clone();

        let (tx, rx) = mpsc::channel::<PathBuf>();
        self.updated = Some(rx);

        let queue = Arc::new(Mutex::new(VecDeque::from(targets)));
        self.queue = Some(queue.clone());
        self.active_kind = kind;
        let service = self.service.clone();
        let batch_counter = self.batch.clone();
        let my_batch = batch_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let workers = WORKERS.min(total).max(1);
        let remaining_workers = Arc::new(Mutex::new(workers));

        for _ in 0..workers {
            let queue = queue.clone();
            let service = service.clone();
            let cancel = cancel.clone();
            let state = state.clone();
            let tx = tx.clone();
            let remaining = remaining_workers.clone();
            let batch_counter = batch_counter.clone();
            std::thread::spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let next = queue.lock().unwrap().pop_front();
                    let Some(game) = next else {
                        break;
                    };
                    {
                        let mut st = state.lock().unwrap();
                        st.current = game.name().to_owned();
                    }
                    let outcome = service.process_game(&game, kind, replace, &cancel);
                    if outcome.box_saved || outcome.cart_saved {
                        let _ = tx.send(game.path.clone());
                    }
                    let mut st = state.lock().unwrap();
                    st.done += 1;
                    if outcome.box_saved {
                        st.saved_box += 1;
                    }
                    if outcome.cart_saved {
                        st.saved_cart += 1;
                    }
                    if outcome.missing {
                        st.missing += 1;
                    }
                    if let Some(err) = outcome.error {
                        st.errors += 1;
                        st.last_error = Some(err);
                    }
                }
                let mut left = remaining.lock().unwrap();
                *left -= 1;
                if *left == 0 {
                    {
                        let mut st = state.lock().unwrap();
                        st.running = false;
                        st.current.clear();
                        if cancel.load(Ordering::Relaxed) {
                            st.stopped = true;
                        }
                    }
                    // Free in-memory LaunchBox/Libretro/No-Intro index maps once no newer batch is
                    // using them, and return freed heap pages to the OS on 1 GiB handhelds.
                    if batch_counter.load(Ordering::Relaxed) == my_batch {
                        service.clear_caches();
                        crate::platform::trim_memory();
                    }
                }
            });
        }
    }

    /// Appends `targets` to the active batch if one is already running with the same `kind`,
    /// otherwise starts a new non-replacing batch. Returns whether any work was queued.
    pub fn enqueue(&mut self, label: String, targets: Vec<Game>, kind: ArtKind) -> bool {
        if targets.is_empty() {
            return false;
        }
        if !self.cancel.load(Ordering::Relaxed) && self.active_kind == kind {
            if let Some(queue) = &self.queue {
                let mut st = self.state.lock().unwrap();
                if st.running {
                    let mut q = queue.lock().unwrap();
                    let mut added = 0usize;
                    for g in targets {
                        if !q.iter().any(|existing| existing.path == g.path) {
                            q.push_back(g);
                            added += 1;
                        }
                    }
                    st.total += added;
                    return added > 0;
                }
            }
        }
        self.start(label, targets, kind, false);
        true
    }

    /// Game paths whose artwork just finished saving since the last frame.
    pub fn drain_updated(&mut self) -> Vec<PathBuf> {
        let Some(rx) = &self.updated else {
            return Vec::new();
        };
        rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_pill_formats_running_and_completed_batches() {
        assert_eq!(Snapshot::default().pill(), None);

        let mut snap = Snapshot {
            running: true,
            total: 10,
            done: 3,
            ..Default::default()
        };
        assert_eq!(snap.pill(), Some(("Artwork  3/10".into(), Some(0.3))));

        snap.saved_box = 2;
        snap.saved_cart = 1;
        assert_eq!(
            snap.pill(),
            Some(("Artwork  3/10  \u{b7}  3 saved".into(), Some(0.3)))
        );

        snap.running = false;
        snap.done = 10;
        assert_eq!(
            snap.pill(),
            Some(("Artwork  \u{b7}  Saved 2 box, 1 cart".into(), None))
        );
    }
}
