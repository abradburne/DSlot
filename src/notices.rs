//! Notices nibOS leaves for the shelf: each file under `/run/nibos/notice/` (a tmpfs) is one,
//! its text shown over the shelf until any button, like the notice of a game that stopped.
//! nibOS's `root-verify` writes one there at every start while the card is written badly, for
//! instance. Each file is shown once per run of DSlot, by its name and modification time (a
//! file written again is news again), and again at the next start, since what it says holds
//! until it is put right. The folder is looked at when the shelf comes up and every few
//! seconds while it is showing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Where nibOS puts them.
pub const DIR: &str = "/run/nibos/notice";

/// How often the folder is looked at while the shelf is up.
const EVERY: Duration = Duration::from_secs(3);

/// The folder of notices, and which have been shown this run.
#[derive(Debug)]
pub struct Notices {
    dir: PathBuf,
    seen: HashSet<(String, Option<SystemTime>)>,
    looked: Option<Instant>,
}

impl Notices {
    /// The system's folder: nibOS's; nowhere on the stock OS or a desktop preview, where
    /// nothing is ever found.
    pub fn system() -> Notices {
        let dir = if cfg!(feature = "nibos") && crate::platform::on_device() {
            PathBuf::from(DIR)
        } else {
            PathBuf::new()
        };
        Notices::in_dir(dir)
    }

    pub fn in_dir(dir: PathBuf) -> Notices {
        Notices {
            dir,
            seen: HashSet::new(),
            looked: None,
        }
    }

    /// The next notice not yet shown this run, if it is time to look and there is one: its
    /// text, marked as shown. Files are taken in name order.
    pub fn next(&mut self) -> Option<String> {
        if self.looked.is_some_and(|at| at.elapsed() < EVERY) {
            return None;
        }
        self.looked = Some(Instant::now());
        self.next_now()
    }

    /// As `next`, whether or not it is time to look.
    pub fn next_now(&mut self) -> Option<String> {
        if self.dir.as_os_str().is_empty() {
            return None;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_none_or(|e| e != "tmp"))
            .collect();
        files.sort();
        for path in files {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            let key = (name, modified);
            if self.seen.contains(&key) {
                continue;
            }
            self.seen.insert(key);
            if let Some(text) = read(&path) {
                eprintln!("dslot: notice {}: {}", path.display(), first_line(&text));
                return Some(text);
            }
        }
        None
    }
}

/// A notice file's text: trimmed, with blank lines between paragraphs kept; none if it is
/// empty or unreadable.
pub fn read(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn each_file_once_a_run_by_name_and_time() {
        let dir = TempDir::new("notices");
        let mut notices = Notices::in_dir(dir.path().to_path_buf());
        assert_eq!(notices.next_now(), None);
        std::fs::write(dir.path().join("root-verify"), "Bad card\n\nresult=bad\n").unwrap();
        std::fs::write(dir.path().join("a-first"), "  First  \n").unwrap();
        std::fs::write(dir.path().join("empty"), "\n\n").unwrap();
        std::fs::write(dir.path().join("later.tmp"), "half written").unwrap();
        // In name order; an empty file and one still being written are passed over.
        assert_eq!(notices.next_now().as_deref(), Some("First"));
        assert_eq!(
            notices.next_now().as_deref(),
            Some("Bad card\n\nresult=bad")
        );
        assert_eq!(notices.next_now(), None);
        // Written again, later: news again. (The clock's resolution: set the time by hand.)
        let path = dir.path().join("root-verify");
        std::fs::write(&path, "Bad card, still\n").unwrap();
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(notices.next_now().as_deref(), Some("Bad card, still"));
        assert_eq!(notices.next_now(), None);
        // The next run (a new Notices) shows what is there again.
        let mut again = Notices::in_dir(dir.path().to_path_buf());
        assert_eq!(again.next_now().as_deref(), Some("First"));
        assert_eq!(again.next_now().as_deref(), Some("Bad card, still"));
        // `next` only looks every few seconds, from the first look on.
        let mut timed = Notices::in_dir(dir.path().to_path_buf());
        assert_eq!(timed.next().as_deref(), Some("First"));
        assert_eq!(timed.next(), None);
    }

    #[test]
    fn nowhere_finds_nothing() {
        let mut none = Notices::in_dir(PathBuf::new());
        assert_eq!(none.next_now(), None);
        let mut gone = Notices::in_dir(PathBuf::from("/nonexistent/notice"));
        assert_eq!(gone.next_now(), None);
        assert_eq!(read(Path::new("/nonexistent/notice/x")), None);
    }
}
