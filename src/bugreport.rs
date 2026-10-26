//! Sending a bug report from the shelf's "the game stopped" notice, on nibOS: `nib bug-report
//! send --why TEXT` (nibOS's `hw/files/usr/lib/nibos/nib-bug-report`) writes a scrubbed copy
//! of this start's log to the card as `nibos/bug-report-DATE.txt` and posts it to the bug
//! tracker. It prints `saved: PATH` first, then `sent: report code XXXXXXXX` and exits 0; or
//! says on stderr what went wrong and exits 3 (no tracker configured), 4 (no network, after
//! trying to bring it up) or 5 (the send failed). It is run on a thread, since it may take up
//! to a minute, and the outcome comes back through a channel.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

/// The command that makes and sends the report, and nibOS's script behind it.
const NIB: &str = "/usr/bin/nib";
const SCRIPT: &str = "/usr/lib/nibos/nib-bug-report";

/// Whether a report can be sent from here: nibOS, with its bug-report command installed.
/// Never on the stock OS or a desktop preview.
pub fn available() -> bool {
    cfg!(feature = "nibos") && executable(Path::new(SCRIPT)) && executable(Path::new(NIB))
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// How sending went.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Sent: the report's code, eight characters, to quote when asking about it.
    Sent { code: String },
    /// No network: the report is on the card, at `file` (relative to the card's root).
    NoNetwork { file: Option<String> },
    /// Not sent (no tracker set, or the send failed): on the card, at `file`.
    Failed { file: Option<String> },
}

impl Outcome {
    /// What the notice says.
    pub fn words(&self) -> String {
        let saved = |file: &Option<String>| match file {
            Some(f) => format!("Saved on the card as {f}"),
            None => "Saved on the card".to_owned(),
        };
        match self {
            Outcome::Sent { .. } => "Sent. Report code".to_owned(),
            Outcome::NoNetwork { file } => format!("No network. {}", saved(file)),
            Outcome::Failed { file } => format!("Could not send. {}", saved(file)),
        }
    }

    /// The report's code, when it was sent: shown large, to read and copy by hand.
    pub fn code(&self) -> Option<&str> {
        match self {
            Outcome::Sent { code } => Some(code),
            _ => None,
        }
    }
}

/// The outcome the command's exit status and output stand for.
pub fn parse(status: Option<i32>, stdout: &str) -> Outcome {
    let after = |prefix: &str| {
        stdout
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix(prefix))
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    // `saved: /mnt/roms/nibos/bug-report-DATE.txt`: as the player finds it on the card.
    let file = after("saved:").map(|path| {
        let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
        parts.reverse();
        parts.join("/")
    });
    match (status, after("sent: report code")) {
        (Some(0), Some(code)) => Outcome::Sent {
            code: code.chars().take(8).collect(),
        },
        (Some(4), _) => Outcome::NoNetwork { file },
        _ => Outcome::Failed { file },
    }
}

/// Makes and sends a report titled `why`, on a thread; the outcome arrives on the receiver.
/// The command's own output goes to DSlot's stderr.
pub fn send(why: &str) -> Receiver<Outcome> {
    let (tx, rx) = mpsc::channel();
    let why = why.to_owned();
    std::thread::spawn(move || {
        eprintln!("dslot: nib bug-report send --why {why:?}");
        let outcome = match Command::new(NIB)
            .args(["bug-report", "send", "--why", &why])
            .output()
        {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout
                    .lines()
                    .chain(String::from_utf8_lossy(&out.stderr).lines())
                {
                    eprintln!("{line}");
                }
                eprintln!("dslot: nib bug-report exited with {}", out.status);
                parse(out.status.code(), &stdout)
            }
            Err(e) => {
                eprintln!("dslot: cannot run {NIB}: {e}");
                Outcome::Failed { file: None }
            }
        };
        // The shelf may have gone (a game started) before the command came back.
        let _ = tx.send(outcome);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_from_the_commands_output() {
        let saved = "saved: /mnt/roms/nibos/bug-report-20261026-120218.txt\n";
        let sent = format!("{saved}sent: report code 3fa9c0de\n");
        assert_eq!(
            parse(Some(0), &sent),
            Outcome::Sent {
                code: "3fa9c0de".into()
            }
        );
        // A code longer than eight characters is cut to eight.
        assert_eq!(
            parse(Some(0), "sent: report code 0123456789abcdef").code(),
            Some("01234567")
        );
        let file = Some("nibos/bug-report-20261026-120218.txt".to_owned());
        assert_eq!(
            parse(Some(4), saved),
            Outcome::NoNetwork { file: file.clone() }
        );
        assert_eq!(
            parse(Some(3), saved),
            Outcome::Failed { file: file.clone() }
        );
        assert_eq!(
            parse(Some(5), saved),
            Outcome::Failed { file: file.clone() }
        );
        // Exit 0 without a code, or killed: not sent either.
        assert_eq!(
            parse(Some(0), saved),
            Outcome::Failed { file: file.clone() }
        );
        assert_eq!(parse(None, ""), Outcome::Failed { file: None });
        // What the notice says.
        assert_eq!(
            parse(Some(4), saved).words(),
            "No network. Saved on the card as nibos/bug-report-20261026-120218.txt"
        );
        assert_eq!(
            parse(Some(5), "").words(),
            "Could not send. Saved on the card"
        );
        assert_eq!(parse(Some(0), &sent).words(), "Sent. Report code");
    }
}
