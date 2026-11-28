//! Receiving ROMs over LocalSend (nibOS): DSlot's Network → Receive games section starts and
//! stops `nibos-drop.service` (allowed for the wheel group by `50-nibos-drop.rules`), configures
//! its PIN and destination card through `nibos-drop config`, and reads `/run/nibos/drop/status.json`.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where `nibos-drop` writes its live status.
pub const STATUS: &str = "/run/nibos/drop/status.json";
/// Persistent settings written by `nibos-drop config`.
pub const CONFIG: &str = "/var/lib/nibos/drop/config.toml";
/// The unit installed by `nibos-hw`.
const UNIT: &str = "/usr/lib/systemd/system/nibos-drop.service";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ActiveTransfer {
    pub file: String,
    pub received: u64,
    pub total: u64,
}

impl ActiveTransfer {
    pub fn percent(&self) -> u8 {
        (self.received * 100)
            .checked_div(self.total)
            .unwrap_or(0)
            .min(100) as u8
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Counts {
    pub accepted: u64,
    pub rejected: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RecentEvent {
    pub time: String,
    pub file: String,
    pub outcome: String,
    pub console: String,
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Status {
    pub active: bool,
    pub alias: String,
    pub port: u16,
    pub pin_required: bool,
    pub target: String,
    pub target_label: String,
    pub free_bytes: u64,
    pub transfer: Option<ActiveTransfer>,
    pub counts: Counts,
    pub recent: Vec<RecentEvent>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub alias: String,
    pub target: String,
    pub pin: String,
    #[serde(default = "default_true")]
    pub auto_artwork: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            alias: String::new(),
            target: String::new(),
            pin: String::new(),
            auto_artwork: true,
        }
    }
}

impl Config {
    pub fn target_mode(&self) -> &str {
        match self.target.as_str() {
            "tf1" | "tf2" => &self.target,
            _ => "auto",
        }
    }
}

/// Optional status file override (`DSLOT_DROP_STATUS`) for desktop previewing and tests.
fn preview() -> Option<PathBuf> {
    std::env::var_os("DSLOT_DROP_STATUS").map(PathBuf::from)
}

/// Whether this system has nibOS's LocalSend ROM receiver installed.
pub fn there() -> bool {
    preview().is_some() || (crate::platform::on_device() && Path::new(UNIT).is_file())
}

/// Reads the receiver's live status (`None` when not written yet).
pub fn status() -> Option<Status> {
    let path = preview().unwrap_or_else(|| PathBuf::from(STATUS));
    parse_status(&std::fs::read_to_string(path).ok()?)
}

/// Reads the persistent config (`Default` when absent).
pub fn config() -> Config {
    let Ok(text) = std::fs::read_to_string(CONFIG) else {
        return Config::default();
    };
    parse_config(&text).unwrap_or_default()
}

pub fn parse_status(text: &str) -> Option<Status> {
    serde_json::from_str(text).ok()
}

pub fn parse_config(text: &str) -> Option<Config> {
    toml::from_str(text).ok()
}

/// Starts `nibos-drop.service` in the background.
pub fn start() {
    systemctl("start");
}

/// Stops `nibos-drop.service` in the background.
pub fn stop() {
    systemctl("stop");
}

fn systemctl(verb: &str) {
    match Command::new("systemctl")
        .args([verb, "--no-block", "nibos-drop.service"])
        .status()
    {
        Ok(s) if s.success() => eprintln!("dslot: drop: {verb} nibos-drop.service"),
        Ok(s) => eprintln!("dslot: drop: systemctl {verb}: {s}"),
        Err(e) => eprintln!("dslot: drop: systemctl: {e}"),
    }
}

/// Sets or clears the LocalSend PIN and restarts the daemon if it is currently active.
pub fn set_pin(pin: Option<&str>, active: bool) {
    let arg = pin.unwrap_or("off");
    run_config(&["--pin", arg], active);
}

/// Sets the destination card (`auto`, `tf1`, `tf2`) and restarts the daemon if active.
pub fn set_target(target: &str, active: bool) {
    run_config(&["--target", target], active);
}

/// Enables or disables automatic box + cart artwork fetching for received ROMs.
pub fn set_auto_artwork(on: bool) {
    run_config(&["--artwork", if on { "on" } else { "off" }], false);
}

/// Paths of recently accepted ROMs from `/run/nibos/drop/status.json`.
pub fn recent_accepted_paths() -> std::collections::HashSet<PathBuf> {
    let Some(s) = status() else {
        return std::collections::HashSet::new();
    };
    let mut paths = std::collections::HashSet::new();
    for ev in &s.recent {
        if ev.outcome != "accepted" {
            continue;
        }
        if !ev.path.is_empty() {
            paths.insert(PathBuf::from(&ev.path));
        } else if !s.target.is_empty() && !ev.console.is_empty() && !ev.file.is_empty() {
            paths.insert(Path::new(&s.target).join(&ev.console).join(&ev.file));
        }
    }
    paths
}

fn run_config(args: &[&str], active: bool) {
    let mut cmd = Command::new("nibos-drop");
    cmd.arg("config").args(args);
    match cmd.status() {
        Ok(s) if s.success() => {
            if active {
                systemctl("restart");
            }
        }
        Ok(s) => eprintln!("dslot: drop: nibos-drop config: {s}"),
        Err(e) => eprintln!("dslot: drop: nibos-drop config: {e}"),
    }
}

/// Picks a 4-digit numeric PIN for a LocalSend session.
pub fn random_pin() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1234, |d| {
            (d.subsec_nanos() ^ d.as_secs() as u32) % 9000 + 1000
        });
    format!("{n:04}")
}

/// Human-readable free space label (`"44.8 GB free"`, `"512 MB free"`).
pub fn format_free(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB free", bytes as f64 / 1_000_000_000.0)
    } else {
        format!("{} MB free", bytes / 1_000_000)
    }
}

/// Cycles `auto` -> `tf2` -> `tf1` -> `auto` (or backwards when `dir < 0`).
pub fn step_target(current: &str, dir: i32) -> &'static str {
    const ORDER: [&str; 3] = ["auto", "tf2", "tf1"];
    let at = ORDER.iter().position(|&t| t == current).unwrap_or(0) as i32;
    ORDER[(at + dir).rem_euclid(ORDER.len() as i32) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_and_config() {
        let s = parse_status(
            r#"{
              "active": true,
              "alias": "RG DS+",
              "port": 53317,
              "pin_required": true,
              "target": "/mnt/ds/Roms",
              "target_label": "TF2 card",
              "free_bytes": 48200000000,
              "transfer": {"file": "Chrono.sfc", "received": 200, "total": 400},
              "counts": {"accepted": 2, "rejected": 1},
              "recent": [{"time": "12:00:00", "file": "Chrono.sfc", "outcome": "accepted", "console": "SFC", "reason": ""}]
            }"#,
        )
        .unwrap();
        assert!(s.active);
        assert_eq!(s.alias, "RG DS+");
        assert_eq!(s.transfer.as_ref().unwrap().percent(), 50);
        assert_eq!(format_free(s.free_bytes), "48.2 GB free");

        let cfg = parse_config("alias = \"RG DS+\"\ntarget = \"tf2\"\npin = \"4242\"\n").unwrap();
        assert_eq!(cfg.target_mode(), "tf2");
        assert_eq!(cfg.pin, "4242");
        assert_eq!(step_target("auto", 1), "tf2");
        assert_eq!(step_target("tf2", 1), "tf1");
        assert_eq!(step_target("tf1", 1), "auto");
        assert_eq!(step_target("auto", -1), "tf1");
    }
}
