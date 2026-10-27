//! The System page's **Date and time** page (nibOS): the time, the date and the time zone, where
//! the time came from, and setting all three. nibOS's clock is systemd's: systemd-timesyncd
//! sets it from the network whenever the network is up, which on a device that brings Wi-Fi up
//! on demand may be long after power-on, so the page says plainly whether the time was set from
//! the network, offers **Sync now** (Wi-Fi up if it is off, then the time servers), a zone
//! picker (region, then place), and setting the time by hand (year, month, day, hour and minute
//! with the D-pad), which turns network time off until **Use network time** turns it back on.
//!
//! All of it goes through nibOS's `nib clock` (`/usr/lib/nibos/nib-clock`): `status` as any
//! user, read on a thread every second while the page is up ([`Watch`]); `sync`, `zone`, `set`
//! and `ntp` as root through `sudo -n`, which nibOS's sudoers lets the wheel group do, each on
//! a thread of its own ([`run`]), as they take up to half a minute. DSlot never sets the clock
//! itself. On the stock OS and a desktop preview there is no row for it ([`available`]).

// The stock OS never reads a status or a zone list; the parsers stay for the tests.
#![cfg_attr(not(feature = "nibos"), allow(dead_code))]

use std::fmt;

/// nibOS's script, run as root through sudo for anything but reading.
pub const NIB_CLOCK: &str = "/usr/lib/nibos/nib-clock";

/// Whether the page is offered here: nibOS, with `nib clock` installed. Never on the stock OS
/// or a desktop preview.
pub fn available() -> bool {
    cfg!(feature = "nibos") && {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(NIB_CLOCK)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
}

/// A date and time, as the clock shows it: local, to the second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Stamp {
    /// `YYYY-MM-DD HH:MM:SS` (or without the seconds).
    pub fn parse(text: &str) -> Option<Stamp> {
        let (date, time) = text.trim().split_once(' ')?;
        let mut d = date.split('-').map(|p| p.parse::<u32>().ok());
        let mut t = time.split(':').map(|p| p.parse::<u32>().ok());
        let stamp = Stamp {
            year: d.next()?? as i32,
            month: d.next()??,
            day: d.next()??,
            hour: t.next()??,
            minute: t.next()??,
            second: t.next().flatten().unwrap_or(0),
        };
        let ok = (1..=12).contains(&stamp.month)
            && (1..=31).contains(&stamp.day)
            && stamp.hour < 24
            && stamp.minute < 60
            && stamp.second < 60;
        ok.then_some(stamp)
    }

    /// The time of day, with seconds: "11:07:33", or "11:07:33 AM" on a 12-hour clock.
    pub fn time(&self, hour12: bool) -> String {
        if !hour12 {
            return format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second);
        }
        let half = if self.hour < 12 { "AM" } else { "PM" };
        let h = match self.hour % 12 {
            0 => 12,
            h => h,
        };
        format!("{h}:{:02}:{:02} {half}", self.minute, self.second)
    }

    /// The time to the minute, for a line that also names the zone.
    pub fn short(&self, hour12: bool) -> String {
        nibkit::clock::format(self.hour, self.minute, hour12)
    }
}

/// What `nib clock status` says, as last read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// Read at least once: before that every field is a blank, not a fact.
    pub read: bool,
    /// The local date and time.
    pub local: Option<Stamp>,
    /// The day's and the month's names, as the system spells them ("Tuesday", "October").
    pub weekday: String,
    pub month: String,
    /// The zone's name ("Asia/Tokyo") and its offset from UTC ("+0900").
    pub zone: String,
    pub offset: String,
    /// Network time (systemd-timesyncd) is on; and the clock is synchronised from it.
    pub ntp: Option<bool>,
    pub synced: Option<bool>,
    /// When the time servers last answered this run, if they have.
    pub last_sync: Option<Stamp>,
    /// The board has a hardware clock.
    pub rtc: bool,
    /// There is a default route.
    pub network: Option<bool>,
}

/// `nib clock status`'s key=value lines.
pub fn parse_status(text: &str) -> Info {
    let mut info = Info {
        read: true,
        ..Info::default()
    };
    let yes = |v: &str| Some(v == "yes");
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "local" => info.local = Stamp::parse(value),
            "weekday" => info.weekday = value.to_owned(),
            "month" => info.month = value.to_owned(),
            "zone" => info.zone = value.to_owned(),
            "offset" => info.offset = value.to_owned(),
            "ntp" => info.ntp = yes(value),
            "synced" => info.synced = yes(value),
            "last_sync" => info.last_sync = Stamp::parse(value),
            "rtc" => info.rtc = value == "yes",
            "network" => info.network = yes(value),
            _ => {}
        }
    }
    info
}

impl Info {
    /// The date in words: "Tuesday 27 October 2026".
    pub fn date_line(&self) -> Option<String> {
        let t = self.local?;
        let month = if self.month.is_empty() {
            MONTHS.get(t.month as usize - 1).copied().unwrap_or("?")
        } else {
            self.month.as_str()
        };
        Some(if self.weekday.is_empty() {
            format!("{} {month} {}", t.day, t.year)
        } else {
            format!("{} {} {month} {}", self.weekday, t.day, t.year)
        })
    }

    /// The zone with its offset: "Asia/Tokyo (+09:00)".
    pub fn zone_line(&self) -> String {
        match (self.zone.as_str(), offset_colon(&self.offset)) {
            ("", _) => "\u{2013}".into(),
            (zone, Some(off)) => format!("{zone} ({off})"),
            (zone, None) => zone.to_owned(),
        }
    }

    /// Where the time came from, and whether that is a worry: "Set from the network", "Not
    /// synchronised yet", "Network time is off". With the last answer's time where it helps.
    pub fn source(&self, hour12: bool) -> (String, bool) {
        if !self.read {
            return ("\u{2013}".into(), false);
        }
        let last = self
            .last_sync
            .map(|t| format!(" (last at {})", t.short(hour12)))
            .unwrap_or_default();
        match (self.ntp, self.synced) {
            (Some(false), _) => ("Network time is off: set by hand".into(), false),
            (_, Some(true)) => (format!("Set from the network{last}"), false),
            (Some(true), _) => (format!("Not set from the network yet{last}"), true),
            _ => ("Unknown".into(), false),
        }
    }

    /// The System row's glance: "11:07, Asia/Tokyo".
    pub fn glance(&self, hour12: bool) -> Option<String> {
        let t = self.local?;
        Some(if self.zone.is_empty() {
            t.short(hour12)
        } else {
            format!("{}, {}", t.short(hour12), self.zone)
        })
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "+0900" as "+09:00".
fn offset_colon(offset: &str) -> Option<String> {
    let (sign, digits) = offset.split_at_checked(1)?;
    if !matches!(sign, "+" | "-")
        || digits.len() != 4
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(format!("{sign}{}:{}", &digits[..2], &digits[2..]))
}

// --- The zones -------------------------------------------------------------------------------

/// The regions a zone is picked from, in the order shown: the continents and oceans of
/// `timedatectl list-timezones`, not the legacy names (US/Pacific, Japan, Etc/GMT+9).
pub const REGIONS: [&str; 10] = [
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
];

/// The zones to choose from, by region, each as its full name ("Asia/Tokyo").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Zones {
    pub regions: Vec<(String, Vec<String>)>,
}

impl Zones {
    /// `timedatectl list-timezones`' lines, grouped under [`REGIONS`] (the others left out).
    pub fn parse(text: &str) -> Zones {
        let mut regions: Vec<(String, Vec<String>)> = REGIONS
            .iter()
            .map(|r| ((*r).to_owned(), Vec::new()))
            .collect();
        for line in text.lines().map(str::trim) {
            let Some((region, _)) = line.split_once('/') else {
                continue;
            };
            if let Some((_, zones)) = regions.iter_mut().find(|(r, _)| r == region) {
                zones.push(line.to_owned());
            }
        }
        regions.retain(|(_, zones)| !zones.is_empty());
        for (_, zones) in &mut regions {
            zones.sort();
        }
        Zones { regions }
    }

    /// A region's zones.
    pub fn of(&self, region: &str) -> &[String] {
        self.regions
            .iter()
            .find(|(r, _)| r == region)
            .map_or(&[], |(_, z)| z.as_slice())
    }
}

/// A zone as a place: "America/Argentina/Buenos_Aires" as "Argentina, Buenos Aires".
pub fn place(zone: &str) -> String {
    let rest = zone.split_once('/').map_or(zone, |(_, rest)| rest);
    rest.replace('/', ", ").replace('_', " ")
}

// --- Setting the time by hand ------------------------------------------------------------------

/// A field of the time being set by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

impl Field {
    pub const ALL: [Field; 5] = [
        Field::Year,
        Field::Month,
        Field::Day,
        Field::Hour,
        Field::Minute,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Year => "Year",
            Field::Month => "Month",
            Field::Day => "Day",
            Field::Hour => "Hour",
            Field::Minute => "Minute",
        }
    }
}

/// The time being set by hand: each field stepped with the D-pad, wrapping round, the day kept
/// within the month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draft {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            year: 2026,
            month: 1,
            day: 1,
            hour: 12,
            minute: 0,
        }
    }
}

impl Draft {
    /// Starts from the time as it is.
    pub fn from(stamp: Option<Stamp>) -> Draft {
        match stamp {
            Some(t) => Draft {
                year: t.year.clamp(YEARS.0, YEARS.1),
                month: t.month,
                day: t.day,
                hour: t.hour,
                minute: t.minute,
            },
            None => Draft::default(),
        }
    }

    /// Steps `field` by `dir` (left or right), round the ends.
    pub fn step(&mut self, field: Field, dir: i32) {
        let wrap =
            |v: i64, lo: i64, hi: i64| lo + (v - lo + i64::from(dir)).rem_euclid(hi - lo + 1);
        match field {
            Field::Year => {
                self.year =
                    wrap(i64::from(self.year), i64::from(YEARS.0), i64::from(YEARS.1)) as i32
            }
            Field::Month => self.month = wrap(i64::from(self.month), 1, 12) as u32,
            Field::Day => {
                self.day = wrap(
                    i64::from(self.day),
                    1,
                    i64::from(days_in(self.year, self.month)),
                ) as u32;
            }
            Field::Hour => self.hour = wrap(i64::from(self.hour), 0, 23) as u32,
            Field::Minute => self.minute = wrap(i64::from(self.minute), 0, 59) as u32,
        }
        self.day = self.day.min(days_in(self.year, self.month));
    }

    /// A field as shown: the month by name, the rest as numbers.
    pub fn show(&self, field: Field, hour12: bool) -> String {
        match field {
            Field::Year => self.year.to_string(),
            Field::Month => MONTHS[self.month as usize - 1].to_owned(),
            Field::Day => self.day.to_string(),
            Field::Hour if hour12 => {
                let half = if self.hour < 12 { "AM" } else { "PM" };
                let h = match self.hour % 12 {
                    0 => 12,
                    h => h,
                };
                format!("{h} {half}")
            }
            Field::Hour => format!("{:02}", self.hour),
            Field::Minute => format!("{:02}", self.minute),
        }
    }

    /// What `nib clock set` takes: "YYYY-MM-DD HH:MM".
    pub fn argument(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// The years the picker turns through.
const YEARS: (i32, i32) = (2000, 2099);

/// The days in a month.
pub fn days_in(year: i32, month: u32) -> u32 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// --- The commands --------------------------------------------------------------------------------

/// Something `nib clock` does as root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// The network up if it is off, then the time servers.
    Sync,
    /// The time zone, by its full name.
    Zone(String),
    /// The time by hand, "YYYY-MM-DD HH:MM"; network time goes off.
    Set(String),
    /// Network time on or off.
    Ntp(bool),
}

impl Do {
    /// `nib clock`'s arguments.
    pub fn args(&self) -> Vec<String> {
        match self {
            Do::Sync => vec!["sync".into()],
            Do::Zone(z) => vec!["zone".into(), z.clone()],
            Do::Set(t) => vec!["set".into(), t.clone()],
            Do::Ntp(on) => vec!["ntp".into(), (if *on { "on" } else { "off" }).into()],
        }
    }
}

/// How a command went: a few words for its row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done(String),
    Failed(String),
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Done(s) | Outcome::Failed(s) => f.write_str(s),
        }
    }
}

/// The outcome from the command's exit status and output: its own words for the common
/// failures (4: no network; 5: the servers didn't answer), the stderr's last line otherwise.
pub fn parse_outcome(what: &Do, status: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    let last = |text: &str| {
        text.lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.strip_prefix("nib clock: ").unwrap_or(l).to_owned())
    };
    match status {
        Some(0) => Outcome::Done(match what {
            Do::Sync => "Set from the network".into(),
            Do::Zone(_) => "Zone set".into(),
            Do::Set(_) => "Set".into(),
            Do::Ntp(true) if stdout.contains("synced:") => "On, set from the network".into(),
            Do::Ntp(true) if stdout.contains("no network") => {
                "On: it syncs when Wi-Fi is up".into()
            }
            Do::Ntp(true) => "On, not synced yet".into(),
            Do::Ntp(false) => "Off".into(),
        }),
        Some(4) => Outcome::Failed("No network: turn Wi-Fi on, or set the time by hand".into()),
        Some(5) => Outcome::Failed("The time servers didn't answer: try again".into()),
        None => Outcome::Failed("Stopped".into()),
        Some(_) => Outcome::Failed(last(stderr).unwrap_or_else(|| "Could not do it".into())),
    }
}

#[cfg(feature = "nibos")]
pub use real::{run, zones, Watch};
#[cfg(not(feature = "nibos"))]
pub use stub::{run, zones, Watch};

#[cfg(not(feature = "nibos"))]
mod stub {
    //! The stock OS: no `nib clock`, so nothing to read or run.
    #![allow(dead_code)]
    use super::{Do, Info, Outcome, Zones};
    use std::sync::mpsc::{self, Receiver};

    pub struct Watch;

    impl Watch {
        pub fn start() -> Option<Watch> {
            None
        }
        pub fn now(&self) -> Info {
            Info::default()
        }
        pub fn poke(&self) {}
    }

    pub fn zones() -> Zones {
        Zones::default()
    }

    pub fn run(what: Do) -> Receiver<Outcome> {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(Outcome::Failed(format!("Not here: {what:?}")));
        rx
    }
}

#[cfg(feature = "nibos")]
mod real {
    use super::{parse_outcome, parse_status, Do, Info, Outcome, Zones, NIB_CLOCK};
    use std::process::Command;
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    /// How often the status is read again while the page is up: the seconds tick.
    const EVERY: Duration = Duration::from_secs(1);

    struct Shared {
        info: Mutex<Info>,
        /// Set to stop the thread; the condvar wakes it, for that or for a read now.
        stop: Mutex<bool>,
        wake: Condvar,
    }

    /// The status, read on a thread every second for as long as it is held.
    pub struct Watch {
        shared: Arc<Shared>,
    }

    impl Watch {
        /// Starts reading; `None` only if the thread can't be started.
        pub fn start() -> Option<Watch> {
            let shared = Arc::new(Shared {
                info: Mutex::new(Info::default()),
                stop: Mutex::new(false),
                wake: Condvar::new(),
            });
            let worker = shared.clone();
            std::thread::Builder::new()
                .name("clock".into())
                .spawn(move || loop {
                    let found = status();
                    *lock(&worker.info) = found;
                    let stop = lock(&worker.stop);
                    let (stop, _) = worker
                        .wake
                        .wait_timeout(stop, EVERY)
                        .unwrap_or_else(|e| e.into_inner());
                    if *stop {
                        break;
                    }
                })
                .ok()?;
            Some(Watch { shared })
        }

        /// The status as last read.
        pub fn now(&self) -> Info {
            lock(&self.shared.info).clone()
        }

        /// Reads again now (after a command), rather than at the next tick.
        pub fn poke(&self) {
            self.shared.wake.notify_all();
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            *lock(&self.shared.stop) = true;
            self.shared.wake.notify_all();
        }
    }

    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `nib clock status`, as any user.
    fn status() -> Info {
        match Command::new(NIB_CLOCK).arg("status").output() {
            Ok(out) => parse_status(&String::from_utf8_lossy(&out.stdout)),
            Err(e) => {
                eprintln!("dslot: clock: cannot run {NIB_CLOCK}: {e}");
                Info::default()
            }
        }
    }

    /// `nib clock zones`: the zones to choose from, by region.
    pub fn zones() -> Zones {
        match Command::new(NIB_CLOCK).arg("zones").output() {
            Ok(out) => Zones::parse(&String::from_utf8_lossy(&out.stdout)),
            Err(e) => {
                eprintln!("dslot: clock: cannot run {NIB_CLOCK}: {e}");
                Zones::default()
            }
        }
    }

    /// Runs `nib clock` as root (`sudo -n`), on a thread; the outcome arrives on the receiver.
    /// The command's own output goes to DSlot's stderr.
    pub fn run(what: Do) -> Receiver<Outcome> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let args = what.args();
            eprintln!("dslot: nib clock {}", args.join(" "));
            let outcome = match Command::new("sudo")
                .arg("-n")
                .arg(NIB_CLOCK)
                .args(&args)
                .output()
            {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    for line in stdout.lines().chain(stderr.lines()) {
                        eprintln!("{line}");
                    }
                    eprintln!("dslot: nib clock exited with {}", out.status);
                    parse_outcome(&what, out.status.code(), &stdout, &stderr)
                }
                Err(e) => {
                    eprintln!("dslot: cannot run sudo {NIB_CLOCK}: {e}");
                    Outcome::Failed("Could not run nib clock".into())
                }
            };
            // The menu may have closed meanwhile.
            let _ = tx.send(outcome);
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str =
        "local=2026-10-27 11:07:33\nweekday=Tuesday\nmonth=October\noffset=+0900\n\
                          zone=Asia/Tokyo\nntp=yes\nsynced=yes\nlast_sync=2026-10-27 10:47:39\n\
                          rtc=yes\nnetwork=yes\n";

    #[test]
    fn the_status_as_shown() {
        let info = parse_status(STATUS);
        assert!(info.read);
        assert_eq!(info.local.unwrap().time(false), "11:07:33");
        assert_eq!(info.local.unwrap().time(true), "11:07:33 AM");
        assert_eq!(info.date_line().as_deref(), Some("Tuesday 27 October 2026"));
        assert_eq!(info.zone_line(), "Asia/Tokyo (+09:00)");
        assert_eq!(
            info.source(false),
            ("Set from the network (last at 10:47)".into(), false)
        );
        assert_eq!(info.glance(true).as_deref(), Some("11:07 AM, Asia/Tokyo"));
        assert!(info.rtc);
        assert_eq!(info.network, Some(true));

        // Not synchronised yet, nothing heard from the servers: a worry.
        let info = parse_status("local=2026-10-27 00:02:00\nzone=UTC\noffset=+0000\nntp=yes\nsynced=no\nrtc=no\nnetwork=no\n");
        assert_eq!(
            info.source(false),
            ("Not set from the network yet".into(), true)
        );
        assert_eq!(info.zone_line(), "UTC (+00:00)");
        assert_eq!(info.date_line().as_deref(), Some("27 October 2026"));
        // Set by hand: network time off, and not a worry.
        let info = parse_status("ntp=no\nsynced=no\n");
        assert_eq!(
            info.source(false),
            ("Network time is off: set by hand".into(), false)
        );
        assert_eq!(info.local, None);
        assert_eq!(Info::default().source(false).0, "\u{2013}");
    }

    #[test]
    fn stamps() {
        let t = Stamp::parse("2026-02-03 04:05").unwrap();
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute, t.second),
            (2026, 2, 3, 4, 5, 0)
        );
        assert_eq!(Stamp::parse("2026-13-03 04:05"), None);
        assert_eq!(Stamp::parse("garbage"), None);
        let noon = Stamp::parse("2026-01-01 12:00:00").unwrap();
        assert_eq!(noon.time(true), "12:00:00 PM");
        let midnight = Stamp::parse("2026-01-01 00:30:00").unwrap();
        assert_eq!(midnight.time(true), "12:30:00 AM");
        assert_eq!(midnight.short(false), "00:30");
    }

    #[test]
    fn zones_by_region() {
        let z = Zones::parse(
            "Africa/Abidjan\nAmerica/Argentina/Buenos_Aires\nAmerica/New_York\nAsia/Tokyo\n\
             Etc/GMT+9\nJapan\nUS/Pacific\nUTC\nEurope/London\nAsia/Kolkata\n",
        );
        let regions: Vec<&str> = z.regions.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(regions, ["Africa", "America", "Asia", "Europe"]);
        assert_eq!(z.of("Asia"), ["Asia/Kolkata", "Asia/Tokyo"]);
        assert_eq!(z.of("Antarctica"), Vec::<String>::new());
        assert_eq!(
            place("America/Argentina/Buenos_Aires"),
            "Argentina, Buenos Aires"
        );
        assert_eq!(place("Asia/Tokyo"), "Tokyo");
        assert_eq!(place("UTC"), "UTC");
    }

    #[test]
    fn the_draft_steps_round() {
        let mut d = Draft::from(Stamp::parse("2026-10-31 23:59:00"));
        d.step(Field::Minute, 1);
        assert_eq!((d.hour, d.minute), (23, 0));
        d.step(Field::Hour, 1);
        assert_eq!(d.hour, 0);
        d.step(Field::Day, 1);
        assert_eq!(d.day, 1);
        d.step(Field::Day, -1);
        assert_eq!(d.day, 31);
        // Into a shorter month the day is kept within it.
        d.step(Field::Month, 1);
        assert_eq!((d.month, d.day), (11, 30));
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        d.step(Field::Month, -1);
        assert_eq!((d.month, d.day), (2, 28));
        d.step(Field::Year, -2);
        assert_eq!(d.year, 2024);
        d.step(Field::Day, 1);
        assert_eq!(d.day, 29, "2024 is a leap year");
        d.step(Field::Year, 1);
        assert_eq!((d.year, d.day), (2025, 28));
        assert_eq!(d.argument(), "2025-02-28 00:00");
        assert_eq!(d.show(Field::Month, false), "February");
        assert_eq!(d.show(Field::Hour, true), "12 AM");
        assert_eq!(d.show(Field::Minute, false), "00");
        let mut y = Draft {
            year: 2099,
            ..Draft::default()
        };
        y.step(Field::Year, 1);
        assert_eq!(y.year, 2000);
        assert_eq!(Draft::from(None), Draft::default());
    }

    #[test]
    fn outcomes_in_words() {
        assert_eq!(
            parse_outcome(&Do::Sync, Some(0), "synced: 2026-10-27 11:07:33 JST\n", ""),
            Outcome::Done("Set from the network".into())
        );
        assert_eq!(
            parse_outcome(
                &Do::Sync,
                Some(4),
                "",
                "nib clock: no network: turn Wi-Fi on\n"
            ),
            Outcome::Failed("No network: turn Wi-Fi on, or set the time by hand".into())
        );
        assert_eq!(
            parse_outcome(&Do::Sync, Some(5), "", ""),
            Outcome::Failed("The time servers didn't answer: try again".into())
        );
        assert_eq!(
            parse_outcome(
                &Do::Zone("Mars/Olympus".into()),
                Some(2),
                "",
                "nib clock: no time zone 'Mars/Olympus' (nib clock zones lists them)\n"
            ),
            Outcome::Failed("no time zone 'Mars/Olympus' (nib clock zones lists them)".into())
        );
        assert_eq!(
            parse_outcome(&Do::Ntp(true), Some(0), "ntp: on, no network yet\n", ""),
            Outcome::Done("On: it syncs when Wi-Fi is up".into())
        );
        assert_eq!(
            parse_outcome(
                &Do::Ntp(true),
                Some(0),
                "ntp: on, synced: 2026-10-27 11:07:33 JST\n",
                ""
            ),
            Outcome::Done("On, set from the network".into())
        );
        assert_eq!(
            parse_outcome(
                &Do::Set("2026-01-01 00:00".into()),
                Some(1),
                "",
                "sudo: a password is required\n"
            ),
            Outcome::Failed("sudo: a password is required".into())
        );
        assert_eq!(
            Do::Set("2026-01-01 00:00".into()).args(),
            ["set", "2026-01-01 00:00"]
        );
        assert_eq!(Do::Ntp(false).args(), ["ntp", "off"]);
    }
}
