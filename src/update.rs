//! System updates (nibOS): the menu's Update page reads what nibOS's updater is doing and
//! starts its steps. The work is nibOS's (`nibos-update@<step>.service`, which the wheel group
//! may start without a password): DSlot never writes a partition and never runs as root.
//!
//! - **Steps**: `check`, `download` (in the background; stopping it pauses, starting it again
//!   resumes), `apply` (restarts into the update), `rollback`, `ack` (the one-time notices
//!   seen), `channel-stable`, `channel-beta`.
//! - **Status**: `/run/nibos/update/status.json`, replaced whole by every step and after every
//!   chunk of a download.
//!
//! DSlot keeps one thing of its own, in its data folder: the notes of the update it is about to
//! restart into (they're gone from the status once it has).

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the updater writes how things are.
pub const STATUS: &str = "/run/nibos/update/status.json";
/// The updater's steps, `nibos-update@<step>.service`.
const UNIT: &str = "/usr/lib/systemd/system/nibos-update@.service";

/// How the updater says things are.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Status {
    /// idle, checking, current, available, downloading, paused, finishing, ready, restarting,
    /// trying or error.
    pub phase: String,
    /// What went wrong (an error), or why there is nothing newer: the updater's own words, which
    /// may be a tool's raw output. Shown only when it is plain words ([`Status::message`]).
    pub detail: String,
    /// What went wrong, or why there is nothing newer, as a stable code (`no_network`,
    /// `not_found`, ...): what DSlot words it from.
    pub code: String,
    /// The version running.
    pub version: String,
    pub edition: String,
    /// stable or beta.
    pub channel: String,
    /// The version there is to update to.
    pub available: String,
    /// What's new in it.
    pub notes: String,
    /// Its download's size, in bytes.
    pub size: u64,
    /// The download's chunks done, of `total`.
    pub done: u64,
    pub total: u64,
    /// Updated to this version (once: until `ack`).
    pub updated_to: String,
    /// The update to this version didn't start, and the device went back (once: until `ack`).
    pub failed: String,
    /// The step that wrote this status: on an error, the one that failed.
    pub step: String,
    /// The version in the other slot (the one before the last update), while it is there.
    pub previous: String,
    /// Whether that slot is known good: going back to it is offered.
    pub rollback_ok: bool,
}

impl Status {
    /// The phase as the page treats it (the updater's own).
    pub fn shown_phase(&self) -> &str {
        &self.phase
    }

    /// Why there is nothing newer, or what went wrong, for a person: DSlot's words for each of
    /// the updater's codes (nibOS's docs/design/updates.md), the updater's detail for a code
    /// it doesn't know (only where it is plain words, never a URL or a tool's output), else a
    /// line saying the journal has it. `None` where there is nothing to say.
    pub fn message(&self) -> Option<String> {
        let here = format!("nibOS {}", self.version);
        let line = match self.code.as_str() {
            // Nothing newer.
            "none_published" => format!("No updates published yet ({here})"),
            "up_to_date" | "older" => format!("You're up to date ({here})"),
            "needs_intermediate" => {
                "This version is too old to update: write the new image to the card".into()
            }
            "not_for_device" => "The newest update isn't for this device".into(),
            // The network.
            "no_network" => "Can't reach the update server: check Wi-Fi".into(),
            "tls_error" => {
                "Couldn't connect securely: check the date and time, then try again".into()
            }
            "not_found" => "The update wasn't found on the server: try again later".into(),
            "rate_limited" => "The update server is busy: try again in a while".into(),
            "server_error" => "The update server had a problem: try again later".into(),
            "http_error" | "network_error" => {
                "Couldn't download from the update server: try again later".into()
            }
            // The release.
            "bad_signature" => "The update didn't check out, so it wasn't used".into(),
            "bad_manifest" => "The update's description couldn't be read: try again later".into(),
            "damaged" => "The download was damaged: try again".into(),
            // The device.
            "low_battery" => "Charge to 30% or plug the charger in, then try again".into(),
            "not_confirmed" => "Update once nibOS has started properly".into(),
            "no_slot" | "no_second_slot" => {
                "This card can't be updated: it has no second system".into()
            }
            "no_space" | "slot_too_small" => {
                "This card's second system is too small for the update".into()
            }
            "card_error" => "The card couldn't be written: it may be failing".into(),
            "no_rollback" => "There's no earlier version to go back to".into(),
            "nothing_pending" | "not_written" => {
                "Nothing is ready to install: check for updates again".into()
            }
            "no_update_source" | "no_update_key" => "Updates aren't set up on this system".into(),
            // Anything else (io_error among them): the updater's own words.
            _ if !self.detail.is_empty() && plain(&self.detail) => self.detail.clone(),
            "" if self.phase == "current" => format!("You're up to date ({here})"),
            "" if self.phase == "error" => {
                "Something went wrong: the journal has the details".into()
            }
            "" => return None,
            code => format!("Something went wrong ({code}): the journal has the details"),
        };
        Some(line)
    }

    /// The download's progress, in percent.
    pub fn percent(&self) -> u8 {
        (self.done * 100)
            .checked_div(self.total)
            .unwrap_or(0)
            .min(100) as u8
    }

    /// The download's size in megabytes, as a person would round it.
    pub fn megabytes(&self) -> u64 {
        (self.size + 500_000) / 1_000_000
    }
}

/// A status file to show in place of the updater's (`DSLOT_UPDATE_STATUS`): the Update page in
/// a desktop preview, to see it in each phase.
fn preview() -> Option<PathBuf> {
    std::env::var_os("DSLOT_UPDATE_STATUS").map(PathBuf::from)
}

/// Whether this system has nibOS's updater.
pub fn there() -> bool {
    preview().is_some() || (crate::platform::on_device() && Path::new(UNIT).is_file())
}

/// How things are now: `None` while the updater has said nothing (as `idle`).
pub fn status() -> Option<Status> {
    let path = preview().unwrap_or_else(|| PathBuf::from(STATUS));
    parse(&std::fs::read_to_string(path).ok()?)
}

/// Whether the updater's detail is words for a person, not a tool's output (a URL, curl's).
fn plain(detail: &str) -> bool {
    let lower = detail.to_lowercase();
    !["http", "curl", "[", "{", "://"]
        .iter()
        .any(|k| lower.contains(k))
}

fn parse(text: &str) -> Option<Status> {
    serde_json::from_str(text).ok()
}

/// Whether there is a one-time notice to show (updated, or the update didn't start): the menu
/// opens on the Update page for it when the shelf comes up.
pub fn notice() -> bool {
    there() && status().is_some_and(|s| !s.updated_to.is_empty() || !s.failed.is_empty())
}

/// Starts a step, in the background (`systemctl --no-block`): how it goes is in the status.
pub fn start(step: &str) {
    let unit = format!("nibos-update@{step}.service");
    systemctl("start", &unit);
}

/// Pauses a download (it resumes where it was when `download` starts again).
pub fn pause() {
    systemctl("stop", "nibos-update@download.service");
}

fn systemctl(verb: &str, unit: &str) {
    match Command::new("systemctl")
        .args([verb, "--no-block", unit])
        .status()
    {
        Ok(s) if s.success() => eprintln!("dslot: update: {verb} {unit}"),
        Ok(s) => eprintln!("dslot: update: systemctl {verb} {unit}: {s}"),
        Err(e) => eprintln!("dslot: update: systemctl: {e}"),
    }
}

/// DSlot's own notes about updates, in its data folder.
fn kept(name: &str) -> Option<PathBuf> {
    Some(nibkit::paths::AppDirs::new("dslot").data.join(name))
}

/// Keeps the notes of the update about to be restarted into, for "What's new" after it.
pub fn keep_notes(s: &Status) {
    if let Some(path) = kept("update-notes.txt") {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(path, format!("{}\n{}", s.available, s.notes));
    }
}

/// The notes kept for `version`, if they are its.
pub fn notes_for(version: &str) -> Option<String> {
    let text = std::fs::read_to_string(kept("update-notes.txt")?).ok()?;
    let (v, notes) = text.split_once('\n')?;
    (v == version).then(|| notes.to_owned())
}

/// The notes as lines of at most `width` characters, broken between words.
pub fn wrap(notes: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for para in notes.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_status() {
        let s = parse(
            r#"{"phase":"downloading","detail":"","version":"0.1.0-alpha1","edition":"dslot",
                "channel":"stable","available":"0.1.0-alpha2","notes":"What's new","size":375155770,
                "done":37,"total":115,"step":"download","previous":"0.1.0-alpha0",
                "rollback_ok":true,"something":"new"}"#,
        )
        .unwrap();
        assert_eq!(s.phase, "downloading");
        assert_eq!((s.percent(), s.megabytes()), (32, 375));
        assert_eq!(s.updated_to, "");
        assert_eq!((s.step.as_str(), s.rollback_ok), ("download", true));
        // Fields left out are empty; a broken file is no status.
        assert_eq!(parse(r#"{"phase":"idle"}"#).unwrap().total, 0);
        assert_eq!(parse(r#"{"phase":"idle"}"#).unwrap().percent(), 0);
        assert!(parse("{").is_none());
    }

    /// A code is worded; a raw error is never shown; an unknown code is the updater's words.
    #[test]
    fn words_errors() {
        let s = |phase: &str, code: &str, detail: &str| Status {
            phase: phase.into(),
            code: code.into(),
            detail: detail.into(),
            version: "0.1.0-alpha1".into(),
            ..Status::default()
        };
        assert_eq!(
            s("current", "none_published", "").message().unwrap(),
            "No updates published yet (nibOS 0.1.0-alpha1)"
        );
        assert_eq!(
            s("error", "no_network", "No network").message().unwrap(),
            "Can't reach the update server: check Wi-Fi"
        );
        assert_eq!(
            s("error", "io_error", "Writing the slot failed.")
                .message()
                .unwrap(),
            "Writing the slot failed."
        );
        // An older updater's raw text stays out.
        let raw = "https://api.github.com/repos/x/y/releases [] curl: (22) error: 404";
        let message = s("error", "", raw).message().unwrap();
        assert!(
            !message.contains("http") && !message.contains("curl"),
            "{message}"
        );
        assert!(s("error", "weird", raw)
            .message()
            .unwrap()
            .contains("weird"));
        assert_eq!(
            s("current", "", "").message().unwrap(),
            "You're up to date (nibOS 0.1.0-alpha1)"
        );
        assert_eq!(s("available", "", "").message(), None);
    }

    #[test]
    fn wraps_notes() {
        let lines = wrap(
            "Faster boot.\n\nA keyboard for the console, and more fixes",
            20,
        );
        assert_eq!(
            lines,
            [
                "Faster boot.",
                "",
                "A keyboard for the",
                "console, and more",
                "fixes"
            ]
        );
    }
}
