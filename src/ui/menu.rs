//! The menu, on the touch screen while the top screen keeps showing the selected game. It has
//! a page for each level of settings (settings/mod.rs):
//!
//! - **Game**: the selected game's favourite and rating, the emulator it starts in and that
//!   emulator's options for it, and actions on its files (its resume state, its saves).
//! - **The console** (DS, GBA, ...): the emulator its games start in, that emulator's options,
//!   and how the page is shown.
//! - **System**: how every page is shown, the clicks, button repeat, the device's volume,
//!   brightness and Wi-Fi where DSlot can change them, start at power-on, power off, and
//!   **System info**: a read-only page of the device and what it is using (`sysinfo.rs`), and
//!   **Date and time** (nibOS): the clock, its zone and where the time came from, with Sync
//!   now, a zone picker and setting it by hand through `nib clock` (`clock.rs`).
//! - **Wi-Fi** and **Bluetooth** (nibOS): the networks in range, joining one (its passphrase
//!   typed on nibkit's keyboard), leaving and forgetting; Bluetooth on and off, the paired
//!   devices to connect and disconnect, forgetting one, and pairing a new one. The work is done
//!   on a thread of its own (`radios.rs`). The Wi-Fi page shows the IPv4 address too, and opens
//!   **Network info** (its row, or Y): a read-only page of what a player is asked for when
//!   something won't connect (`netinfo.rs`).
//! - **Update** (nibOS): the version, the update channel, checking for an update, downloading
//!   it in the background (paused and resumed), restarting into it, what's new, and going back
//!   to the version before; the work is nibOS's updater's (`update.rs`).
//! - **Achievements** (where there is `curl`): the RetroAchievements sign-in, kept for the
//!   emulators (`achievements.rs`): the username and password typed on the keyboard, who is
//!   signed in and their points, signing out, hardcore or softcore, and the sign-up page as a
//!   QR code for a phone, since the service has no way to make an account from here.
//!
//! A row that can inherit shows where its value comes from ("from GBA", "from System") or that
//! it is set here; Y resets it to what it inherits. L / R or the tabs change page; up and down
//! pick a row, left and right or A (or a tap) change it, B closes the menu.

use super::clock;
use super::netinfo;
use super::radios::{Ask, Do, Radios, Reply, Snapshot};
use super::sysinfo;
use crate::achievements::{self, Account, Outcome, Standing};
use crate::emulator::{Action, Emulator, Emulators};
use crate::launch;
use crate::platform::{self, Control};
use crate::rom::{Console, Game};
use crate::settings::def::{self, Def, Kind, Levels};
use crate::settings::{console_chain, game_chain, Scope, Settings};
use crate::stats::Stats;
use crate::update::{self, Status};
use nibkit::menu::keyboard::{self, Keyboard, Purpose, Typed};
use std::time::{Duration, Instant};
use toml::Value as Toml;

// Where the rows and tabs are: nibkit's menu layout, which draws them too.
use nibkit::menu::layout::{row_hit, tab_hit, VISIBLE};
#[cfg(test)]
use nibkit::menu::layout::{row_y, ROW_STEP, TABS, TOP};
/// How often the System page reads the battery again while it is shown.
const BATTERY_EVERY: Duration = Duration::from_secs(3);
/// How often the Update page reads the updater's status again while it is shown.
const UPDATE_EVERY: Duration = Duration::from_millis(500);
/// What's new, broken into lines of this many characters.
const NOTES_WIDTH: usize = 64;

/// A page of the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Game,
    Console,
    System,
    Wifi,
    Bluetooth,
    Update,
    /// RetroAchievements (achievements.rs).
    Achievements,
}

/// What a row is.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Favourite,
    Rating,
    /// Kept in the settings, at the page's level.
    Setting(Def),
    /// The emulator the page's games start in.
    Emulator,
    /// One of the device's own.
    Control(Control),
    /// The battery's charge and what it is doing; shown, not changed.
    Battery,
    PowerOnStart,
    /// The speaker amps' tuning, nibOS's or the official one (nibkit::audio).
    SpeakerTuning,
    /// Imports the official tuning from the user's stock firmware; asks first.
    ImportTuning,
    /// Restarts, for a change that applies then; asks first.
    RestartNow,
    /// The clock, 12-hour or 24-hour: nibOS's preference, for every app.
    Clock,
    /// System (nibOS): opens the Date and time page (clock.rs).
    DateTime,
    /// A row of the Date and time page.
    ClockRow(ClockRow),
    /// The zone picker: a region (or UTC); A opens its places.
    ZoneRegion(String),
    /// The zone picker: a place, by its zone's full name; A sets it.
    Zone {
        id: String,
        label: String,
    },
    /// Setting the time by hand: a field, stepped with left and right.
    ClockField(clock::Field),
    /// Setting the time by hand: A sets the clock to the fields.
    ClockApply,
    /// Looks in every console's folders again, for games the watch on them missed.
    Rescan,
    /// System: opens the System info page (sysinfo.rs).
    SystemInfo,
    /// A line of the System info page; shown, not changed.
    Sys(SysRow),
    /// An emulator's action on the game's files; asks before it acts.
    Action(Action),
    /// Deletes the game's saves folder; asks before it acts.
    DeleteSaves,
    PowerOff,
    /// Wi-Fi: the network joined, or what the Wi-Fi is doing; shown, not changed.
    Joined,
    /// Wi-Fi: the IPv4 address, for the common case; shown, not changed.
    Address,
    /// Wi-Fi: opens the Network info page (netinfo.rs).
    NetworkInfo,
    /// A line of the Network info page; A on any runs the reachability checks again.
    Info(InfoRow),
    /// Wi-Fi: look for networks again.
    Scan,
    /// Wi-Fi: a network in range, by name: A joins it (or leaves it), Y forgets it.
    Network(String),
    /// Bluetooth on or off.
    BluetoothPower,
    /// Bluetooth: a device, by address, with its name: A connects, disconnects or pairs it, Y
    /// forgets it.
    Gadget {
        address: String,
        name: String,
    },
    /// Bluetooth: look for devices to pair.
    PairNew,
    /// The version of nibOS running; shown, not changed.
    UpdateVersion,
    /// The update channel, stable or beta.
    UpdateChannel,
    /// The update: checking, downloading, pausing, restarting into it, as it stands.
    UpdateState,
    /// The notes of the update there is, or just made.
    WhatsNew,
    /// Restarts into the version before the last update; asks first.
    Rollback,
    /// Achievements: the username to sign in as; A types it.
    RaUser,
    /// Achievements: A asks for the password and signs in.
    RaSignIn,
    /// Achievements: who is signed in and their points; shown, not changed.
    RaAccount,
    /// Achievements: signs out; asks first.
    RaSignOut,
    /// Achievements: opens the sign-up page's address and QR code.
    RaSignUp,
    /// The sign-up page's address, over the QR code; shown, not changed.
    RaLink,
    /// A line of What's new; A goes back to the page.
    NoteLine(String),
    /// A line of the update's message, past what fits on the title line.
    Message(String),
}

impl Item {
    pub fn label(&self) -> &str {
        match self {
            Item::Favourite => "Favourite",
            Item::Rating => "Rating",
            Item::Setting(def) => &def.label,
            Item::Emulator => "Emulator",
            Item::Control(c) => c.label(),
            Item::Battery => "Battery",
            Item::PowerOnStart => "Start at power-on",
            Item::Clock => "Clock",
            Item::DateTime => "Date and time",
            Item::ClockRow(row) => row.label(),
            Item::ZoneRegion(region) => region,
            Item::Zone { label, .. } => label,
            Item::ClockField(field) => field.label(),
            Item::ClockApply => "Set the clock",
            Item::Rescan => "Rescan games",
            Item::SystemInfo => "System info",
            Item::Sys(row) => row.label(),
            Item::Action(a) => &a.label,
            Item::DeleteSaves => "Delete saves",
            Item::PowerOff => "Power off",
            Item::SpeakerTuning => "Speaker tuning",
            Item::ImportTuning => "Import official tuning",
            Item::RestartNow => "Restart now",
            Item::Joined => "Joined",
            Item::Address => "Address",
            Item::NetworkInfo => "Network info",
            Item::Info(row) => row.label(),
            Item::Scan => "Look for networks",
            Item::Network(name) => name,
            Item::BluetoothPower => "Bluetooth",
            Item::Gadget { name, .. } => name,
            Item::PairNew => "Pair a new device",
            Item::UpdateVersion => "nibOS",
            Item::UpdateChannel => "Updates",
            Item::UpdateState => "Update",
            Item::WhatsNew => "What's new",
            Item::Rollback => "Go back to the previous version",
            Item::RaUser => "Username",
            Item::RaSignIn => "Sign in",
            Item::RaAccount => "Signed in as",
            Item::RaSignOut => "Sign out",
            Item::RaSignUp => "Make an account",
            Item::RaLink => "On a phone",
            Item::NoteLine(line) | Item::Message(line) => line,
        }
    }

    /// Whether it acts on A, and only once confirmed with A again.
    fn asks(&self) -> bool {
        matches!(
            self,
            Item::Action(_)
                | Item::DeleteSaves
                | Item::PowerOff
                | Item::ImportTuning
                | Item::RestartNow
                | Item::Rollback
                | Item::RaSignOut
        )
    }
}

/// The Network info page's lines, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoRow {
    Wifi,
    Network,
    Address,
    Gateway,
    Dns,
    Name,
    Mac,
    Internet,
    Updates,
    Tracker,
}

impl InfoRow {
    const ALL: [InfoRow; 10] = [
        InfoRow::Wifi,
        InfoRow::Network,
        InfoRow::Address,
        InfoRow::Gateway,
        InfoRow::Dns,
        InfoRow::Name,
        InfoRow::Mac,
        InfoRow::Internet,
        InfoRow::Updates,
        InfoRow::Tracker,
    ];

    fn label(self) -> &'static str {
        match self {
            InfoRow::Wifi => "Wi-Fi",
            InfoRow::Network => "Network",
            InfoRow::Address => "Address",
            InfoRow::Gateway => "Gateway",
            InfoRow::Dns => "DNS",
            InfoRow::Name => "This device",
            InfoRow::Mac => "Wi-Fi MAC",
            InfoRow::Internet => "Internet",
            InfoRow::Updates => "Update server",
            InfoRow::Tracker => "Bug tracker",
        }
    }
}

/// The System info page's lines, in order; a line whose source the device lacks is left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysRow {
    Board,
    Nibos,
    Kernel,
    Uptime,
    Slot,
    Dslot,
    Memory,
    Available,
    DslotMemory,
    Shared,
    Cache,
    Swap,
    Cma,
    Load,
    Cpu,
    Temperature,
    Gpu,
    /// A file system, by the name the page gives it ("System", "NIBOS-DATA", "TF2 card").
    Storage(&'static str),
    Battery,
    Display,
}

impl SysRow {
    /// The lines before the storage, in order.
    const FIRST: [SysRow; 17] = [
        SysRow::Board,
        SysRow::Nibos,
        SysRow::Kernel,
        SysRow::Uptime,
        SysRow::Slot,
        SysRow::Dslot,
        SysRow::Memory,
        SysRow::Available,
        SysRow::DslotMemory,
        SysRow::Shared,
        SysRow::Cache,
        SysRow::Swap,
        SysRow::Cma,
        SysRow::Load,
        SysRow::Cpu,
        SysRow::Temperature,
        SysRow::Gpu,
    ];

    fn label(self) -> &'static str {
        match self {
            SysRow::Board => "Board",
            SysRow::Nibos => "nibOS",
            SysRow::Kernel => "Kernel",
            SysRow::Uptime => "Up for",
            SysRow::Slot => "Slot",
            SysRow::Dslot => "DSlot",
            SysRow::Memory => "Memory",
            SysRow::Available => "Available",
            SysRow::DslotMemory => "DSlot's own",
            SysRow::Shared => "Shared (GPU)",
            SysRow::Cache => "File cache",
            SysRow::Swap => "Swap",
            SysRow::Cma => "CMA",
            SysRow::Load => "Load",
            SysRow::Cpu => "CPU",
            SysRow::Temperature => "Temperature",
            SysRow::Gpu => "GPU",
            SysRow::Storage(name) => name,
            SysRow::Battery => "Battery",
            SysRow::Display => "Display",
        }
    }
}

/// The Date and time page's rows, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockRow {
    Time,
    Date,
    Zone,
    Source,
    Ntp,
    Sync,
    Manual,
}

impl ClockRow {
    const ALL: [ClockRow; 7] = [
        ClockRow::Time,
        ClockRow::Date,
        ClockRow::Zone,
        ClockRow::Source,
        ClockRow::Ntp,
        ClockRow::Sync,
        ClockRow::Manual,
    ];

    fn label(self) -> &'static str {
        match self {
            ClockRow::Time => "Time",
            ClockRow::Date => "Date",
            ClockRow::Zone => "Time zone",
            ClockRow::Source => "Time source",
            ClockRow::Ntp => "Use network time",
            ClockRow::Sync => "Sync now",
            ClockRow::Manual => "Set by hand",
        }
    }
}

/// What the Date and time page shows in place of the System page's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ClockView {
    Closed,
    /// The page itself.
    Page,
    /// The zone picker's regions.
    Regions,
    /// The zone picker's places in this region.
    Places(String),
    /// Setting the time by hand.
    Manual,
}

/// A row's value as shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Stars(u8),
    /// A percentage, drawn as a bar.
    Slider(i64),
    /// What a row inherits, set nowhere here ("Use System (Carts)"): muted.
    Inherited(String),
    /// Words and then small pictures: a network's check, lock and signal, a device's battery.
    Status {
        text: String,
        glyphs: Vec<nibkit::menu::Glyph>,
    },
    /// Text without the arrows that say it changes: an action's prompt (nothing, "Press A", or
    /// the warning that A again does it), or a row only shown (the battery).
    Prompt {
        text: String,
        warning: bool,
    },
}

/// A row as drawn.
pub struct Row {
    pub label: String,
    pub value: Value,
    /// Where an inheriting row's value comes from: "from System", "set here", ...
    pub source: Option<String>,
}

/// The menu as drawn.
pub struct Shown {
    /// Each page's tab, and whether it is the page on show.
    pub tabs: Vec<(String, bool)>,
    /// The line under the tabs: what the page is about.
    pub title: String,
    /// All the page's rows; those from `scroll` on are on screen.
    pub rows: Vec<Row>,
    pub scroll: usize,
    /// The highlighted row, as a place among all of the page's rows.
    pub row: usize,
    /// The buttons' line at the bottom: Y says what the highlighted row would go back to.
    pub hint: String,
    /// The keyboard, over the menu, while something is being typed.
    pub keyboard: Option<Keyboard>,
    /// A QR code under the rows (the sign-up page's address).
    pub picture: Option<achievements::Qr>,
}

/// What a row on a console's or a game's page goes back to when not set there: where from (a
/// level, "default", or the emulator's own), and its value as shown.
struct Inherit {
    set_here: bool,
    from: String,
    shown: Option<String>,
}

impl Inherit {
    /// "Use System (Carts)", "Use GBA (Light)", "Use DSperate's own". A value's own note in
    /// brackets is left out: "Use DS (2x)", not "Use DS (2x (512x384))".
    fn text(&self) -> String {
        match &self.shown {
            Some(v) => {
                let v = v.split(" (").next().unwrap_or(v);
                format!("Use {} ({v})", self.from)
            }
            None => format!("Use {}", self.from),
        }
    }
}

/// A level as a row names it.
fn level_name(scope: &Scope) -> String {
    match scope {
        Scope::System => "System".into(),
        Scope::Console(c) => Console::from_folder(c).map_or(c.clone(), |k| k.label().to_owned()),
        Scope::Game(_) => "the game".into(),
    }
}

/// The values a setting steps through, in order.
fn values(def: &Def) -> Vec<Toml> {
    match &def.kind {
        Kind::Choice(choices) => choices.iter().map(|(v, _)| v.clone()).collect(),
        Kind::Toggle => vec![Toml::Boolean(true), Toml::Boolean(false)],
        Kind::Number { min, max, step, .. } => (*min..=*max)
            .step_by((*step).max(1) as usize)
            .map(Toml::Integer)
            .collect(),
    }
}

/// The menu's input this frame.
#[derive(Default)]
pub struct Input {
    /// Rows to move by.
    pub moved: i32,
    /// Steps to change the value by.
    pub turn: i32,
    /// A or a tap, as against the D-pad: only those may act.
    pub pressed: bool,
    /// Y: back to what the row inherits.
    pub reset: bool,
    /// L / R: pages to move by.
    pub page: i32,
    /// Left or right let go of: a slider has settled.
    pub released: bool,
    pub close: bool,
    /// Where taps came down on the touch screen.
    pub taps: Vec<(f32, f32)>,
    /// Every button pressed, for the keyboard while typing.
    pub keys: Vec<keyboard::Key>,
}

/// What the menu did this frame.
#[derive(Default, PartialEq, Eq, Debug)]
pub struct Done {
    /// A setting, the game's favourite or its rating changed: save and show it.
    pub changed: bool,
    pub closed: bool,
    /// Power off was asked for, and confirmed.
    pub power_off: bool,
    /// A restart was asked for, and confirmed.
    pub restart: bool,
    /// Rescan games was pressed: the shelf asks for it, and tells the menu what it found
    /// (`rescanned`).
    pub rescan: bool,
}

/// What the menu works on.
pub struct Context<'a> {
    pub stats: &'a mut Stats,
    pub settings: &'a mut Settings,
    pub emulators: &'a Emulators,
    /// The shelf page's console.
    pub console: Console,
    /// The selected game, if there is one.
    pub game: Option<&'a Game>,
}

impl Context<'_> {
    /// The levels a row on `page` is looked up at, most particular first.
    fn chain(&self, page: Page) -> Vec<Scope> {
        match (page, self.game) {
            (Page::Game, Some(game)) => game_chain(game).to_vec(),
            (Page::System | Page::Achievements, _) => vec![Scope::System],
            _ => console_chain(self.console).to_vec(),
        }
    }

    /// The emulator the page's games start in.
    /// The emulators a page can choose between: on a game's, those that take that game (one
    /// listing the games it plays takes only those, nibkit::dslot); else the console's.
    fn choices(&self, page: Page) -> Vec<&Emulator> {
        let system = self.console.folder();
        match (page, self.game) {
            (Page::Game, Some(game)) => self
                .emulators
                .for_system(system)
                .filter(|e| e.takes(system, &game.stem))
                .collect(),
            _ => self.emulators.for_system(system).collect(),
        }
    }

    fn emulator(&self, page: Page) -> Option<&Emulator> {
        match (page, self.game) {
            (Page::Game, Some(game)) => launch::emulator_for(self.emulators, self.settings, game),
            _ => {
                let chain = console_chain(self.console);
                self.emulators
                    .choose(self.console.folder(), self.settings.emulator(&chain))
            }
        }
    }
}

/// The open menu.
pub struct Menu {
    pub page: Page,
    /// The highlighted row.
    pub row: usize,
    /// The first row on screen.
    scroll: usize,
    /// The row that has been asked once to act, and wants confirming.
    armed: Option<usize>,
    /// What the last action did, shown on its row until another row is picked.
    note: Option<(usize, String)>,
    /// Whether DSlot starts at power-on, where the menu can change that.
    at_boot: Option<bool>,
    /// The device's controls as the OS has them: read when the menu opens and when the System
    /// page is turned to, and the screens' brightness again whenever it changes (Menu + volume
    /// changes it too). DSlot keeps no level of its own.
    controls: Vec<(Control, Option<i64>)>,
    /// Changes to the screens' brightness and to battery protect, by anyone; none on the
    /// stock OS.
    watches: Vec<nibkit::watch::Watch>,
    /// Changes to the volume, by anyone (the volume keys); none on the stock OS.
    volume: Option<nibkit::device::VolumeWatch>,
    /// The battery's line, and when it was read (the kernel's count changes unseen: read again
    /// every few seconds).
    battery: Option<(String, Instant)>,
    /// A slider moved and not yet let go of.
    moving: Option<Control>,
    /// The speaker amps' tuning, read when the System page is shown (it takes a command).
    tuning: Option<nibkit::audio::Tuning>,
    /// A command running for a row (importing the tuning): the row, and its answer to come.
    job: Option<(Item, std::sync::mpsc::Receiver<std::io::Result<String>>)>,
    /// A change made that applies at the next restart: "Restart now" is offered.
    restart: bool,
    /// The clock is 12-hour (nibOS's preference): read when the menu opens and when the
    /// System page is turned to.
    hour12: bool,
    /// The Wi-Fi and Bluetooth pages' worker (nibOS), and how things were when last asked.
    radios: Option<Radios>,
    radio: Snapshot,
    /// The keyboard, while something is being typed, and what it is for.
    typing: Option<(Keyboard, Typing)>,
    /// The row Y has been pressed on once, to forget a network or device with Y again.
    forget: Option<usize>,
    /// The updater's status (nibOS), and when it was read.
    update: Option<(Status, Instant)>,
    /// The step last started, for Try again after an error.
    update_step: &'static str,
    /// A one-time notice: updated, or the update didn't start. Kept while DSlot runs.
    update_notice: Option<String>,
    /// What's new, for the update there is or the one just made.
    update_notes: Option<(String, String)>,
    /// What's new is open on the Update page, in place of its rows.
    reading_notes: bool,
    /// The network's facts, read on a thread while the Wi-Fi page is up (nibOS).
    netinfo: Option<netinfo::Watch>,
    net: netinfo::Info,
    /// Network info is open on the Wi-Fi page, in place of its rows.
    reading_net: bool,
    /// The clock (nibOS): `nib clock status`, read on a thread while the System page is up.
    clock: Option<clock::Watch>,
    time: clock::Info,
    /// What the Date and time page shows, in place of the System page's rows.
    clock_view: ClockView,
    /// The zones to choose from, read when the picker first opens.
    zones: Option<clock::Zones>,
    /// The time being set by hand.
    draft: clock::Draft,
    /// A `nib clock` command running: its row, and its outcome to come.
    clock_job: Option<(ClockRow, std::sync::mpsc::Receiver<clock::Outcome>)>,
    /// The device's facts, read on a thread while System info is up.
    sysinfo: Option<sysinfo::Watch>,
    sys: sysinfo::Info,
    /// System info is open on the System page, in place of its rows.
    reading_sys: bool,
    /// RetroAchievements: the sign-in kept (achievements.rs), where it is kept, the username
    /// typed so far, what the service said of the player, and a sign-in or check running.
    ra_account: Option<Account>,
    ra_path: std::path::PathBuf,
    ra_user: String,
    ra_standing: Option<Standing>,
    ra_job: Option<(Item, std::sync::mpsc::Receiver<Outcome>)>,
    /// The last outcome's line, for the title.
    ra_said: Option<String>,
    /// The sign-in kept has been checked with the service this time the menu is open.
    ra_checked: bool,
    /// The sign-up page's address and QR code are open, in place of the rows.
    ra_signup: bool,
}

/// What the keyboard is typing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Typing {
    /// The passphrase for this network.
    Passphrase(String),
    /// A code a device being paired asked for.
    Code,
    /// The RetroAchievements username.
    RaUser,
    /// The RetroAchievements password, for this username: signs in when typed.
    RaPassword(String),
}

impl Menu {
    /// Opens on the Game page when there is a game selected, else on the console's.
    pub fn open(game: bool) -> Menu {
        let controls = platform::CONTROLS
            .iter()
            .map(|&c| (c, platform::control(c)))
            .collect();
        let ra_path = platform::dirs().achievements;
        Menu {
            page: if game { Page::Game } else { Page::Console },
            row: 0,
            scroll: 0,
            armed: None,
            note: None,
            at_boot: platform::start_at_power_on(),
            controls,
            watches: [nibkit::backlight::watch(), nibkit::battery::watch()]
                .into_iter()
                .flatten()
                .collect(),
            volume: nibkit::device::watch_volume(),
            battery: None,
            moving: None,
            hour12: nibkit::clock::hour12(),
            tuning: None,
            job: None,
            restart: false,
            radios: Radios::start(),
            radio: Snapshot::default(),
            typing: None,
            forget: None,
            update: None,
            update_step: "check",
            update_notice: None,
            update_notes: None,
            reading_notes: false,
            netinfo: None,
            net: netinfo::Info::default(),
            reading_net: false,
            clock: None,
            time: clock::Info::default(),
            clock_view: ClockView::Closed,
            zones: None,
            draft: clock::Draft::default(),
            clock_job: None,
            sysinfo: None,
            sys: sysinfo::Info::default(),
            reading_sys: false,
            ra_account: Account::load(&ra_path),
            ra_user: Account::load(&ra_path).map(|a| a.user).unwrap_or_default(),
            ra_path,
            ra_standing: None,
            ra_job: None,
            ra_said: None,
            ra_checked: false,
            ra_signup: false,
        }
    }

    /// Opens on the Update page, where nibOS has an updater: for a notice at start-up.
    pub fn open_update(game: bool) -> Menu {
        let mut menu = Menu::open(game);
        if update::there() {
            menu.page = Page::Update;
        }
        menu
    }

    /// Opens on the Bluetooth page, or the Wi-Fi page (a tap on its icon), where there is one.
    pub fn open_radio(game: bool, bluetooth: bool) -> Menu {
        let mut menu = Menu::open(game);
        if menu.radios.is_some() {
            menu.page = if bluetooth {
                Page::Bluetooth
            } else {
                Page::Wifi
            };
        }
        menu
    }

    /// Reads the controls again from the OS: those `which` picks (one being moved is left as
    /// the slider has it).
    fn reread(&mut self, which: impl Fn(Control) -> bool) {
        self.battery = None;
        for (control, value) in &mut self.controls {
            if which(*control) && self.moving != Some(*control) {
                *value = platform::control(*control);
            }
        }
    }

    /// The battery's line: its charge and what it is doing, and whether protect holds it.
    fn battery_line(&self) -> Option<String> {
        let b = nibkit::battery::read()?;
        let protect = self
            .controls
            .iter()
            .any(|&(c, v)| c == Control::BatteryProtect && v == Some(1));
        Some(crate::stats::battery_line(&b, protect))
    }

    /// The pages there are: Game only with a game selected, Wi-Fi and Bluetooth where DSlot can
    /// talk to them (nibOS).
    fn pages(&self, cx: &Context) -> Vec<Page> {
        let game = cx.game.is_some().then_some(Page::Game);
        let radios = self
            .radios
            .is_some()
            .then_some([Page::Wifi, Page::Bluetooth]);
        let updates = update::there().then_some(Page::Update);
        let achievements = achievements::available().then_some(Page::Achievements);
        game.into_iter()
            .chain([Page::Console, Page::System])
            .chain(radios.into_iter().flatten())
            .chain(updates)
            .chain(achievements)
            .collect()
    }

    /// The page's rows.
    pub fn items(&self, cx: &Context) -> Vec<Item> {
        let mut items = Vec::new();
        let choice = cx.choices(self.page).len() > 1;
        let shelf = |items: &mut Vec<Item>| {
            for key in [def::VIEW, def::SORT, def::SHOW, def::CASE] {
                items.extend(def::builtin(key).map(Item::Setting));
            }
        };
        let options = |items: &mut Vec<Item>| {
            if let Some(emulator) = cx.emulator(self.page) {
                items.extend(
                    emulator
                        .options_for(cx.console.folder())
                        .map(|o| Item::Setting(o.def.clone())),
                );
            }
        };
        match self.page {
            Page::Game => {
                items.extend([Item::Favourite, Item::Rating]);
                if choice {
                    items.push(Item::Emulator);
                }
                options(&mut items);
                let root = platform::saves();
                let saves = root.as_deref();
                if let Some(emulator) = cx.emulator(self.page) {
                    let actions = emulator
                        .actions
                        .iter()
                        .filter(|a| a.applies(cx.console.folder()) && launch::can_do(a, saves));
                    items.extend(actions.cloned().map(Item::Action));
                }
                // An app keeps its own files: DSlot has no saves folder for it.
                if saves.is_some() && cx.console != Console::Apps {
                    items.push(Item::DeleteSaves);
                }
            }
            // The apps' page is always a grid, of boxes without cases.
            Page::Console if cx.console == Console::Apps => {
                for key in [def::SORT, def::SHOW] {
                    items.extend(def::builtin(key).map(Item::Setting));
                }
            }
            Page::Console => {
                if choice {
                    items.push(Item::Emulator);
                }
                options(&mut items);
                shelf(&mut items);
                if cx.console == Console::Snes {
                    items.extend(def::builtin(def::SNES_CART).map(Item::Setting));
                }
            }
            Page::System if self.clock_view != ClockView::Closed => self.clock_items(&mut items),
            Page::System if self.reading_sys => {
                items.extend(self.sys_rows().into_iter().map(Item::Sys));
                if items.is_empty() {
                    items.push(Item::Sys(SysRow::Board));
                }
            }
            Page::System => {
                shelf(&mut items);
                let own = [
                    def::SHOW_EMPTY,
                    def::SOUNDS,
                    def::REPEAT,
                    def::SAVER,
                    def::SAVER_LOOK,
                ]
                .into_iter()
                .filter_map(def::builtin);
                items.extend(own.map(Item::Setting));
                let there = self.controls.iter().filter(|(_, v)| v.is_some());
                for &(c, _) in there.filter(|(c, _)| *c != Control::BatteryProtect) {
                    items.push(Item::Control(c));
                }
                // The battery's line, and its protect switch where the system has one.
                if self.battery.is_some() || nibkit::battery::read().is_some() {
                    items.push(Item::Battery);
                }
                let protect =
                    |&(c, v): &(Control, Option<i64>)| c == Control::BatteryProtect && v.is_some();
                if self.controls.iter().any(protect) {
                    items.push(Item::Control(Control::BatteryProtect));
                }
                items.push(Item::Clock);
                if clock::available() {
                    items.push(Item::DateTime);
                }
                items.push(Item::Rescan);
                items.push(Item::SystemInfo);
                if self.tuning.is_some() {
                    items.extend([Item::SpeakerTuning, Item::ImportTuning]);
                }
                if self.at_boot.is_some() {
                    items.push(Item::PowerOnStart);
                }
                if self.restart {
                    items.push(Item::RestartNow);
                }
                items.push(Item::PowerOff);
            }
            Page::Wifi if self.reading_net => {
                items.extend(InfoRow::ALL.into_iter().map(Item::Info));
            }
            Page::Wifi => {
                if self
                    .controls
                    .iter()
                    .any(|(c, v)| *c == Control::Wifi && v.is_some())
                {
                    items.push(Item::Control(Control::Wifi));
                }
                if self.radio.wifi.is_some() {
                    items.extend([Item::Joined, Item::Address]);
                }
                items.push(Item::NetworkInfo);
                if let Some(wifi) = &self.radio.wifi {
                    items.push(Item::Scan);
                    items.extend(wifi.networks.iter().map(|n| Item::Network(n.name.clone())));
                }
            }
            Page::Update if self.reading_notes => {
                let notes = self.update_notes.as_ref().map_or("", |(_, n)| n.as_str());
                items.extend(
                    update::wrap(notes, NOTES_WIDTH)
                        .into_iter()
                        .map(Item::NoteLine),
                );
                if items.is_empty() {
                    items.push(Item::NoteLine(String::new()));
                }
            }
            Page::Update => {
                let s = self.update.as_ref().map(|(s, _)| s);
                items.extend([Item::UpdateVersion, Item::UpdateChannel, Item::UpdateState]);
                // What the title line can't hold of the message, on lines of its own.
                if self.update_notice.is_none() {
                    let message = s.and_then(Status::message).unwrap_or_default();
                    let lines = update::wrap(&message, NOTES_WIDTH);
                    items.extend(lines.into_iter().skip(1).map(Item::Message));
                }
                if self.update_notes.is_some() {
                    items.push(Item::WhatsNew);
                }
                // The other slot, while it is known good (the updater says).
                let back = s.is_some_and(|s| {
                    s.rollback_ok && !s.previous.is_empty() && s.phase != "restarting"
                });
                if back {
                    items.push(Item::Rollback);
                }
            }
            Page::Achievements => self.ra_items(&mut items),
            Page::Bluetooth => {
                items.push(Item::BluetoothPower);
                if let Some(bt) = self.radio.bluetooth.as_ref().filter(|b| b.powered) {
                    let gadget = |g: &super::radios::Gadget| Item::Gadget {
                        address: g.address.clone(),
                        name: g.name.clone(),
                    };
                    items.extend(bt.devices.iter().filter(|g| g.paired).map(gadget));
                    items.push(Item::PairNew);
                    if bt.discovering {
                        items.extend(bt.devices.iter().filter(|g| !g.paired).map(gadget));
                    }
                }
            }
        }
        items
    }

    pub fn update(&mut self, input: &Input, cx: &mut Context) -> Done {
        self.job_done(cx);
        if let Some(radios) = &self.radios {
            self.radio = radios.now();
        }
        // Typing: the keyboard has every button and touch.
        if self.typing.is_some() {
            self.type_on(input);
            return Done::default();
        }
        // A device being paired asks: a code is typed, a yes or no is A or B.
        if let Some(ask) = self.radio.asking.clone() {
            match ask {
                Ask::Pin | Ask::Passkey => {
                    let (title, max) = if ask == Ask::Pin {
                        ("The device's PIN", 16)
                    } else {
                        ("The passkey the device shows", 6)
                    };
                    self.typing = Some((Keyboard::new(title, Purpose::Digits, max), Typing::Code));
                    return Done::default();
                }
                Ask::Confirm(_) | Ask::Allow if input.pressed || input.close => {
                    if let Some(radios) = &self.radios {
                        radios.reply(if input.pressed { Reply::Yes } else { Reply::No });
                    }
                    return Done::default();
                }
                Ask::Confirm(_) | Ask::Allow => return Done::default(),
                Ask::Show(_) => {}
            }
        }
        let mut done = Done {
            closed: input.close,
            ..Done::default()
        };
        let (mut turn, mut pressed, mut page_turn) = (input.turn, input.pressed, input.page);
        let pages = self.pages(cx);
        // A tap on a tab turns to it; one on a row picks it and steps its value.
        for &(x, y) in &input.taps {
            if let Some(p) = tab_hit(x, y, pages.len()) {
                page_turn = 0;
                self.turn_to(pages[p]);
            } else if let Some(r) = row_hit(y).map(|r| r + self.scroll) {
                (self.row, turn, pressed) = (r, turn + 1, true);
            }
        }
        if page_turn != 0 {
            let at = pages.iter().position(|&p| p == self.page).unwrap_or(0) as i32;
            self.turn_to(pages[(at + page_turn).rem_euclid(pages.len() as i32) as usize]);
        }
        if !pages.contains(&self.page) {
            self.turn_to(Page::Console);
        }
        // Drained on every page, so the System page only rereads for changes made while it shows.
        let changed = self
            .watches
            .iter_mut()
            .fold(false, |any, w| w.changed() | any);
        let volume = self.volume.as_mut().is_some_and(|w| w.changed());
        if self.page == Page::System
            && self
                .battery
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() >= BATTERY_EVERY)
        {
            self.battery = self.battery_line().map(|line| (line, Instant::now()));
        }
        if (changed || volume) && self.page == Page::System {
            let was = self.controls.clone();
            // The volume takes a command to read: only when it is what changed.
            self.reread(|c| (changed && c.cheap()) || (volume && c == Control::Volume));
            // Changed elsewhere (the volume keys, Menu + volume, `nib`, another app), not by a
            // slider here.
            for ((control, now), (_, before)) in self.controls.iter().zip(&was) {
                if now != before {
                    let level = match now {
                        Some(v) if control.is_switch() => {
                            (if *v != 0 { "on" } else { "off" }).into()
                        }
                        Some(v) => format!("{v}%"),
                        None => "?".to_owned(),
                    };
                    eprintln!("dslot: {} now {level}", control.label());
                }
            }
        }

        if self.page == Page::Update {
            self.read_update();
        }
        // The network's facts, while the Wi-Fi page is up.
        if self.page == Page::Wifi && self.radios.is_some() {
            if self.netinfo.is_none() {
                self.netinfo = netinfo::Watch::start();
            }
            if let Some(watch) = &self.netinfo {
                self.net = watch.now();
            }
        } else if self.netinfo.take().is_some() {
            self.net = netinfo::Info::default();
        }
        // Network info: B goes back to the Wi-Fi page, A checks again.
        if self.reading_net && input.close {
            self.close_net_info();
            return Done::default();
        }
        // The device's facts, while System info is up.
        if self.page == Page::System && self.reading_sys {
            if self.sysinfo.is_none() {
                self.sysinfo = sysinfo::Watch::start();
            }
            if let Some(watch) = &self.sysinfo {
                self.sys = watch.now();
            }
        } else if self.sysinfo.take().is_some() {
            self.sys = sysinfo::Info::default();
        }
        // The clock, while the System page is up (its row shows the time).
        if self.page == Page::System && clock::available() {
            if self.clock.is_none() {
                self.clock = clock::Watch::start();
            }
            if let Some(watch) = &self.clock {
                self.time = watch.now();
            }
        } else if self.clock.take().is_some() {
            self.time = clock::Info::default();
        }
        self.clock_job_done(cx);
        self.ra_job_done(cx);
        // The sign-up page: B goes back to the Achievements page, on its row.
        if self.ra_signup && input.close {
            self.ra_back(cx);
            return Done::default();
        }
        // Date and time: B goes back a level, to the System page at last.
        if self.clock_view != ClockView::Closed && input.close {
            self.clock_back(cx);
            return Done::default();
        }
        // System info: B goes back to the System page, on its row.
        if self.reading_sys && input.close {
            self.reading_sys = false;
            let items = self.items(cx);
            self.row = items
                .iter()
                .position(|i| *i == Item::SystemInfo)
                .unwrap_or(0);
            self.scroll = self.row.saturating_sub(VISIBLE - 1);
            return Done::default();
        }

        let items = self.items(cx);
        let was = self.row;
        let n = items.len() as i32;
        self.row = ((self.row as i32 + input.moved).rem_euclid(n) as usize).min(items.len() - 1);
        self.scroll = self
            .scroll
            .clamp(self.row.saturating_sub(VISIBLE - 1), self.row);
        if self.row != was {
            (self.armed, self.note, self.forget) = (None, None, None);
        }
        let settled = input.released || self.row != was || input.close;
        if let Some(control) = self.moving.filter(|_| settled) {
            platform::control_settled(control);
            self.moving = None;
        }
        let item = &items[self.row];
        let chosen = pressed && was == self.row;
        // Y on the Wi-Fi page opens Network info, except on a saved network, where it forgets.
        let forgets = matches!(item, Item::Network(name) if self.known(name));
        if self.page == Page::System && self.clock_view != ClockView::Closed {
            let item = item.clone();
            self.clock_row(&item, chosen, turn, cx);
        } else if *item == Item::DateTime {
            if chosen {
                self.open_clock(cx);
            }
        } else if self.page == Page::System && self.reading_sys {
            // Shown only: nothing to press or turn.
        } else if *item == Item::SystemInfo {
            if chosen {
                self.open_sys_info();
            }
        } else if self.page == Page::Wifi && self.reading_net {
            if chosen {
                if let Some(watch) = &self.netinfo {
                    watch.check_again();
                }
            }
        } else if self.page == Page::Wifi
            && ((*item == Item::NetworkInfo && chosen) || (input.reset && !forgets))
        {
            self.open_net_info();
        } else if matches!(self.page, Page::Wifi | Page::Bluetooth)
            && !matches!(item, Item::Control(_))
        {
            let item = item.clone();
            self.radio_row(&item, chosen, turn, input.reset);
        } else if self.page == Page::Update {
            let item = item.clone();
            self.update_row(&item, pressed && was == self.row, turn);
        } else if self.page == Page::Achievements && !matches!(item, Item::Setting(_)) {
            let item = item.clone();
            self.ra_row(&item, chosen);
        } else if *item == Item::Rescan {
            if pressed && was == self.row {
                done.rescan = true;
                self.note = Some((self.row, "Looking\u{2026}".into()));
            }
        } else if item.asks() {
            if pressed && was == self.row {
                if self.armed == Some(self.row) {
                    self.armed = None;
                    match item {
                        Item::PowerOff => {
                            return Done {
                                power_off: true,
                                ..done
                            }
                        }
                        Item::RestartNow => {
                            return Done {
                                restart: true,
                                ..done
                            }
                        }
                        Item::ImportTuning => {
                            self.run(Item::ImportTuning, nibkit::audio::import_tuning)
                        }
                        _ => self.act(item, cx),
                    }
                } else {
                    self.armed = Some(self.row);
                }
            } else if turn != 0 {
                self.armed = None;
            }
        } else if input.reset {
            done.changed = self.reset(item, cx);
        } else if turn != 0 {
            done.changed = self.step(item, turn.signum(), cx);
        }
        done
    }

    /// Reads the updater's status, at most every [`UPDATE_EVERY`]. A one-time notice (updated, or
    /// the update didn't start) is kept for as long as DSlot runs, and acknowledged at once.
    fn read_update(&mut self) {
        if self
            .update
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() < UPDATE_EVERY)
        {
            return;
        }
        let s = update::status().unwrap_or_default();
        if self.update_notice.is_none() && !s.updated_to.is_empty() {
            self.update_notice = Some(format!("Updated to nibOS {}", s.updated_to));
            self.update_notes = update::notes_for(&s.updated_to).map(|n| (s.updated_to.clone(), n));
            update::start("ack");
        } else if self.update_notice.is_none() && !s.failed.is_empty() {
            self.update_notice = Some(format!(
                "The update to nibOS {} didn't start: you're back on {}",
                s.failed, s.version
            ));
            update::start("ack");
        }
        if !s.notes.trim().is_empty() && !s.available.is_empty() {
            self.update_notes = Some((s.available.clone(), s.notes.clone()));
        }
        self.update = Some((s, Instant::now()));
    }

    /// A button on an Update row: `chosen` A (or a tap) on it, `turn` left or right.
    fn update_row(&mut self, item: &Item, chosen: bool, turn: i32) {
        let Some((s, _)) = self.update.clone() else {
            return;
        };
        let go = |step: &'static str, menu: &mut Menu| {
            update::start(step);
            menu.update_step = step;
            menu.update = None;
        };
        match item {
            Item::UpdateChannel if chosen || turn != 0 => {
                let to = if s.channel == "beta" {
                    "channel-stable"
                } else {
                    "channel-beta"
                };
                go(to, self);
            }
            Item::UpdateState if chosen => match s.shown_phase() {
                "" | "idle" | "current" => go("check", self),
                "available" => go("download", self),
                "downloading" => {
                    update::pause();
                    self.update = None;
                }
                "paused" => go("download", self),
                "ready" if self.armed == Some(self.row) => {
                    self.armed = None;
                    update::keep_notes(&s);
                    go("apply", self);
                }
                "ready" => self.armed = Some(self.row),
                // The step that failed, as the updater says, else the one DSlot last started.
                "error" => {
                    let step = match s.step.as_str() {
                        "check" => "check",
                        "download" => "download",
                        "apply" => "apply",
                        "rollback" => "rollback",
                        _ => self.update_step,
                    };
                    go(step, self);
                }
                _ => {}
            },
            Item::WhatsNew if chosen => {
                (self.reading_notes, self.row, self.scroll) = (true, 0, 0);
            }
            Item::NoteLine(_) if chosen => {
                (self.reading_notes, self.row, self.scroll) = (false, 0, 0);
            }
            Item::Rollback if chosen && self.armed == Some(self.row) => {
                self.armed = None;
                go("rollback", self);
            }
            Item::Rollback if chosen => self.armed = Some(self.row),
            _ if turn != 0 => self.armed = None,
            _ => {}
        }
    }

    /// A button on a Wi-Fi or Bluetooth row: `chosen` A (or a tap) on it, `turn` left or right,
    /// `reset` Y.
    fn radio_row(&mut self, item: &Item, chosen: bool, turn: i32, reset: bool) {
        let Some(radios) = &self.radios else { return };
        let snap = &self.radio;
        match item {
            Item::Scan if chosen => radios.ask(Do::Scan),
            Item::Network(name) => {
                let Some(net) = snap
                    .wifi
                    .as_ref()
                    .and_then(|w| w.networks.iter().find(|n| &n.name == name))
                else {
                    return;
                };
                if reset && net.known {
                    if self.forget == Some(self.row) {
                        radios.ask(Do::Forget(name.clone()));
                        self.forget = None;
                    } else {
                        self.forget = Some(self.row);
                    }
                } else if chosen && net.connected {
                    if self.armed == Some(self.row) {
                        radios.ask(Do::Leave);
                        self.armed = None;
                    } else {
                        self.armed = Some(self.row);
                    }
                } else if chosen && net.unsupported {
                    self.note = Some((self.row, "DSlot can't join this kind of network".into()));
                } else if chosen && net.locked {
                    let kb = Keyboard::new(format!("Passphrase for {name}"), Purpose::Secret, 63);
                    self.typing = Some((kb, Typing::Passphrase(name.clone())));
                } else if chosen {
                    radios.ask(Do::Join(name.clone(), None));
                }
            }
            Item::BluetoothPower if chosen || turn != 0 => {
                let on = snap.bluetooth.as_ref().is_some_and(|b| b.powered);
                radios.ask(Do::Power(!on));
            }
            Item::PairNew if chosen => {
                let looking = snap.bluetooth.as_ref().is_some_and(|b| b.discovering);
                radios.ask(Do::Look(!looking));
            }
            Item::Gadget { address, .. } => {
                let Some(g) = snap
                    .bluetooth
                    .as_ref()
                    .and_then(|b| b.devices.iter().find(|g| &g.address == address))
                else {
                    return;
                };
                if reset && g.paired {
                    if self.forget == Some(self.row) {
                        radios.ask(Do::Remove(address.clone()));
                        self.forget = None;
                    } else {
                        self.forget = Some(self.row);
                    }
                } else if chosen {
                    radios.ask(match (g.paired, g.connected) {
                        (false, _) => Do::Pair(address.clone()),
                        (true, true) => Do::Disconnect(address.clone()),
                        (true, false) => Do::Connect(address.clone()),
                    });
                }
            }
            _ => {}
        }
    }

    /// Whether a network in range is a saved one (Y forgets it).
    fn known(&self, name: &str) -> bool {
        self.radio
            .wifi
            .as_ref()
            .is_some_and(|w| w.networks.iter().any(|n| n.name == name && n.known))
    }

    /// Opens Network info over the Wi-Fi page's rows, and runs its checks.
    fn open_net_info(&mut self) {
        (self.reading_net, self.row, self.scroll) = (true, 0, 0);
        (self.armed, self.note, self.forget) = (None, None, None);
        if let Some(watch) = &self.netinfo {
            watch.check_again();
        }
    }

    /// Opens System info over the System page's rows.
    fn open_sys_info(&mut self) {
        (self.reading_sys, self.row, self.scroll) = (true, 0, 0);
        (self.armed, self.note) = (None, None);
    }

    /// Back to the Wi-Fi page's rows, on its Network info row.
    fn close_net_info(&mut self) {
        self.reading_net = false;
        let joined_rows = if self.radio.wifi.is_some() { 2 } else { 0 };
        self.row = usize::from(self.has_wifi_switch()) + joined_rows;
        self.scroll = self.row.saturating_sub(VISIBLE - 1);
    }

    /// Whether the Wi-Fi page starts with the switch's row.
    fn has_wifi_switch(&self) -> bool {
        self.controls
            .iter()
            .any(|(c, v)| *c == Control::Wifi && v.is_some())
    }

    /// Whether the keyboard is up: it takes every button.
    pub fn typing(&self) -> bool {
        self.typing.is_some()
    }

    /// The keyboard's buttons and touches, while typing.
    fn type_on(&mut self, input: &Input) {
        let Some((kb, typing)) = self.typing.as_mut() else {
            return;
        };
        let mut typed = None;
        for &key in &input.keys {
            typed = typed.or(kb.key(key));
        }
        for &(x, y) in &input.taps {
            typed = typed.or(kb.touch(x, y));
        }
        let Some(typed) = typed else { return };
        let typing = typing.clone();
        self.typing = None;
        match (typed, typing) {
            (Typed::Done(user), Typing::RaUser) => {
                self.ra_user = user.trim().to_owned();
            }
            (Typed::Done(pass), Typing::RaPassword(user)) if !pass.is_empty() => {
                self.ra_go(Item::RaSignIn, achievements::sign_in(user, pass));
            }
            (_, Typing::RaUser | Typing::RaPassword(_)) => {}
            (typed, typing) => {
                let Some(radios) = &self.radios else { return };
                match (typed, typing) {
                    (Typed::Done(pass), Typing::Passphrase(name)) => {
                        radios.ask(Do::Join(name, Some(pass)))
                    }
                    (Typed::Done(code), Typing::Code) => radios.reply(Reply::Code(code)),
                    (Typed::Cancel, Typing::Code) => radios.reply(Reply::No),
                    _ => {}
                }
            }
        }
    }

    fn turn_to(&mut self, page: Page) {
        if self.page != page {
            (self.page, self.row, self.scroll, self.armed, self.note) = (page, 0, 0, None, None);
            (self.forget, self.reading_notes, self.reading_net) = (None, false, false);
            self.reading_sys = false;
            self.clock_view = ClockView::Closed;
            self.ra_signup = false;
            if page == Page::Achievements {
                self.ra_check();
            }
            if page == Page::System {
                self.reread(|_| true);
                self.hour12 = nibkit::clock::hour12();
                self.tuning = nibkit::audio::tuning();
            }
        }
    }

    /// The level the page sets values at.
    fn scope(&self, cx: &Context) -> Scope {
        cx.chain(self.page).remove(0)
    }

    /// What `item` goes back to on this page when not set here; `None` for a row that doesn't
    /// inherit (anything on the System page, and what isn't a setting).
    fn inherits(&self, item: &Item, cx: &Context) -> Option<Inherit> {
        if matches!(self.page, Page::System | Page::Achievements) {
            return None;
        }
        let chain = cx.chain(self.page);
        let (here, parents) = (&chain[0], &chain[1..]);
        match item {
            Item::Setting(def) if def.levels.has(here) => {
                let set_here = cx
                    .settings
                    .get(here, &def.key)
                    .is_some_and(|v| def.accepts(v));
                let (from, shown) = match cx.settings.source(parents, def) {
                    Some(level) => (
                        level_name(level),
                        cx.settings.value(parents, def).map(|v| def.show(&v)),
                    ),
                    // An emulator's option set nowhere isn't passed: the emulator's own holds.
                    None if is_option(def) => {
                        let name = cx
                            .emulator(self.page)
                            .map_or("the emulator".to_owned(), |e| e.name.clone());
                        (format!("{name}'s own"), None)
                    }
                    None => ("default".into(), def.default.as_ref().map(|v| def.show(v))),
                };
                Some(Inherit {
                    set_here,
                    from,
                    shown,
                })
            }
            Item::Emulator => {
                let set = |s: &Scope| cx.settings.get(s, def::EMULATOR).is_some();
                let level = parents
                    .iter()
                    .find(|s| Levels::CONSOLE_AND_GAME.has(s) && set(s));
                let wanted = cx.settings.emulator(parents);
                let shown = cx
                    .emulators
                    .choose(cx.console.folder(), wanted)
                    .map(|e| e.name.clone());
                Some(Inherit {
                    set_here: set(here),
                    from: level.map_or("default".into(), level_name),
                    shown,
                })
            }
            _ => None,
        }
    }

    /// Steps a row that inherits: through "Use ..." (set nowhere here) and then its values,
    /// round and round (a number's run from "Use ..." up to its largest and stop).
    fn step_inheriting(&mut self, item: &Item, dir: i32, set_here: bool, cx: &mut Context) {
        let here = self.scope(cx);
        let (key, all, wraps) = match item {
            Item::Setting(def) => (
                def.key.clone(),
                values(def),
                !matches!(def.kind, Kind::Number { .. }),
            ),
            Item::Emulator => (
                def::EMULATOR.to_owned(),
                cx.choices(self.page)
                    .into_iter()
                    .map(|e| Toml::from(e.id.as_str()))
                    .collect(),
                true,
            ),
            _ => return,
        };
        let now = set_here.then(|| cx.settings.get(&here, &key)).flatten();
        let at = now
            .and_then(|v| all.iter().position(|w| w == v))
            .map_or(0, |i| i as i32 + 1);
        let places = all.len() as i32 + 1;
        let next = if wraps {
            (at + dir).rem_euclid(places)
        } else {
            (at + dir).clamp(0, places - 1)
        };
        if next == 0 {
            cx.settings.reset(&here, &key);
        } else {
            cx.settings.set(here, &key, all[next as usize - 1].clone());
        }
    }

    /// Changes `item` by one step in direction `dir`. Returns whether a setting changed.
    fn step(&mut self, item: &Item, dir: i32, cx: &mut Context) -> bool {
        if let Some(inherit) = self.inherits(item, cx) {
            self.step_inheriting(item, dir, inherit.set_here, cx);
            return true;
        }
        let scope = self.scope(cx);
        match item {
            Item::Favourite | Item::Rating => {
                let Some(game) = cx.game else { return false };
                if *item == Item::Favourite {
                    cx.stats.toggle_fave(&game.path);
                } else {
                    let rating = (cx.stats.rating(&game.path) as i32 + dir).rem_euclid(6) as u8;
                    cx.stats.set_rating(&game.path, rating);
                }
            }
            Item::Setting(def) => {
                let now = cx.settings.value(&cx.chain(self.page), def);
                cx.settings
                    .set(scope, &def.key, def.step(now.as_ref(), dir));
            }
            Item::Emulator => {
                let choice: Vec<String> = cx
                    .choices(self.page)
                    .into_iter()
                    .map(|e| e.id.clone())
                    .collect();
                let now = cx.emulator(self.page).map(|e| e.id.clone());
                let at = choice
                    .iter()
                    .position(|id| Some(id) == now.as_ref())
                    .unwrap_or(0) as i32;
                let next = &choice[(at + dir).rem_euclid(choice.len() as i32) as usize];
                cx.settings
                    .set(scope, def::EMULATOR, Toml::from(next.as_str()));
            }
            Item::Control(control) => {
                let Some((_, Some(value))) = self.controls.iter_mut().find(|(c, _)| c == control)
                else {
                    return false;
                };
                *value = if control.is_switch() {
                    1 - *value
                } else {
                    // 5% steps that land on multiples of 5, as nibOS's shortcut steps.
                    i64::from(nibkit::backlight::step((*value).clamp(0, 100) as u8, dir))
                };
                platform::set_control(*control, *value);
                self.moving = Some(*control);
                return false;
            }
            Item::SpeakerTuning => {
                let official = self.tuning != Some(nibkit::audio::Tuning::Official);
                self.run(Item::SpeakerTuning, move || {
                    nibkit::audio::set_tuning(official)
                });
                return false;
            }
            Item::Clock => {
                if let Err(e) = nibkit::clock::set_hour12(!self.hour12) {
                    eprintln!("dslot: clock: {e}");
                }
                self.hour12 = nibkit::clock::hour12();
                return false;
            }
            Item::PowerOnStart => {
                let Some(on) = self.at_boot else { return false };
                if let Err(e) = platform::set_start_at_power_on(!on) {
                    eprintln!("dslot: start at power-on: {e}");
                }
                self.at_boot = platform::start_at_power_on();
                return false;
            }
            Item::Action(_)
            | Item::DeleteSaves
            | Item::PowerOff
            | Item::Battery
            | Item::ImportTuning
            | Item::RestartNow
            | Item::Rescan
            | Item::SystemInfo
            | Item::Sys(_)
            | Item::DateTime
            | Item::ClockRow(_)
            | Item::ZoneRegion(_)
            | Item::Zone { .. }
            | Item::ClockField(_)
            | Item::ClockApply
            | Item::RaUser
            | Item::RaSignIn
            | Item::RaAccount
            | Item::RaSignOut
            | Item::RaSignUp
            | Item::RaLink
            | Item::Joined
            | Item::Address
            | Item::NetworkInfo
            | Item::Info(_)
            | Item::Scan
            | Item::Network(_)
            | Item::BluetoothPower
            | Item::Gadget { .. }
            | Item::PairNew
            | Item::UpdateVersion
            | Item::UpdateChannel
            | Item::UpdateState
            | Item::WhatsNew
            | Item::Rollback
            | Item::NoteLine(_)
            | Item::Message(_) => return false,
        }
        true
    }

    /// Lets the page's level inherit `item` again. Returns whether it had been set here.
    fn reset(&mut self, item: &Item, cx: &mut Context) -> bool {
        let key = match item {
            Item::Setting(def) => def.key.as_str(),
            Item::Emulator => def::EMULATOR,
            _ => return false,
        };
        let scope = self.scope(cx);
        let set = cx.settings.get(&scope, key).is_some();
        cx.settings.reset(&scope, key);
        set
    }

    /// Runs `work` (a command, maybe a slow one) away from the menu, `item`'s row saying it is
    /// working and then what it answered. One at a time.
    fn run(&mut self, item: Item, work: impl FnOnce() -> std::io::Result<String> + Send + 'static) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let what = item.label().to_owned();
        // It carries on if the menu closes meanwhile; what it answered is in the journal.
        std::thread::spawn(move || {
            let answer = work();
            match &answer {
                Ok(text) => eprintln!("dslot: {what}: {text}"),
                Err(e) => eprintln!("dslot: {what}: {e}"),
            }
            let _ = tx.send(answer);
        });
        self.note = Some((self.row, "Working\u{2026}".into()));
        self.job = Some((item, rx));
    }

    /// What Rescan games found: on its row, while it is still the one picked.
    pub fn rescanned(&mut self, (came, went): (usize, usize)) {
        let text = match (came, went) {
            (0, 0) => "No changes".to_owned(),
            (came, 0) => format!("{came} new"),
            (0, went) => format!("{went} gone"),
            (came, went) => format!("{came} new, {went} gone"),
        };
        if let Some((_, note)) = self.note.as_mut().filter(|(_, n)| n.starts_with("Looking")) {
            *note = text;
        }
    }

    /// The running command's answer, when it has come: on its row, and the tuning read again.
    fn job_done(&mut self, cx: &Context) {
        let Some((item, rx)) = &self.job else { return };
        let Ok(answer) = rx.try_recv() else { return };
        let item = item.clone();
        self.job = None;
        let text = match answer {
            Ok(text) => {
                // It applies at the next restart.
                self.restart = true;
                text
            }
            Err(e) => e.to_string(),
        };
        // Read again; a read that fails keeps what was shown.
        if let Some(now) = nibkit::audio::tuning() {
            self.tuning = Some(now);
        }
        let row = self.items(cx).iter().position(|i| *i == item);
        self.note = row.map(|r| (r, text));
    }

    /// Does what an action row says, now that it is confirmed.
    fn act(&mut self, item: &Item, cx: &Context) {
        let root = platform::saves();
        let (Some(game), Some(root)) = (cx.game, root.as_deref()) else {
            return;
        };
        let note = match item {
            Item::Action(action) => match launch::do_action(action, game, Some(root)) {
                0 => "Nothing to delete".to_owned(),
                1 => "1 file deleted".to_owned(),
                n => format!("{n} files deleted"),
            },
            Item::DeleteSaves => match launch::delete_saves(root, game) {
                Ok(()) => "Deleted".to_owned(),
                Err(e) => {
                    eprintln!("dslot: {e}");
                    "Could not delete".to_owned()
                }
            },
            _ => return,
        };
        self.note = Some((self.row, note));
    }

    /// The menu as it is to be drawn.
    pub fn shown(&self, cx: &Context) -> Shown {
        let pages = self.pages(cx);
        let tab = |p: Page| match p {
            Page::Game => "Game".to_owned(),
            Page::Console => cx.console.label().to_owned(),
            Page::System => "System".to_owned(),
            Page::Wifi => "Wi-Fi".to_owned(),
            Page::Bluetooth => "Bluetooth".to_owned(),
            Page::Update => "Update".to_owned(),
            Page::Achievements => "Achievements".to_owned(),
        };
        let tabs = pages.iter().map(|&p| (tab(p), p == self.page)).collect();
        let radio_title = || {
            let r = &self.radio;
            match (&r.asking, &r.busy, &r.said) {
                (Some(Ask::Confirm(k)), ..) => {
                    format!("Does the device show {k:06}?  A yes  \u{b7}  B no")
                }
                (Some(Ask::Allow), ..) => {
                    "Let this device pair without a code?  A yes  \u{b7}  B no".into()
                }
                (Some(Ask::Show(code)), ..) => format!("Type {code} on the device, then Enter"),
                (_, Some(busy), _) => busy.clone(),
                (_, _, Some(said)) if !said.is_empty() => said.clone(),
                _ if self.page == Page::Wifi && self.reading_net => self.net.summary(),
                _ if self.page == Page::Wifi => "Wi-Fi networks".into(),
                _ => "Bluetooth devices".into(),
            }
        };
        let update_title = || {
            let s = self.update.as_ref().map(|(s, _)| s);
            match (&self.update_notice, s) {
                _ if self.reading_notes => match &self.update_notes {
                    Some((v, _)) => format!("What's new in nibOS {v}"),
                    None => "What's new".into(),
                },
                (Some(notice), _) => notice.clone(),
                (_, Some(s)) => match s.shown_phase() {
                    // The message's first line; the rest goes on rows below (Item::Message).
                    "current" | "error" => s
                        .message()
                        .and_then(|m| update::wrap(&m, NOTES_WIDTH).into_iter().next())
                        .unwrap_or_else(|| "nibOS updates".into()),
                    "available" => format!("nibOS {} is available", s.available),
                    "downloading" => format!(
                        "Downloading nibOS {}\u{2026} you can keep playing",
                        s.available
                    ),
                    "paused" => format!("Downloading nibOS {} is paused", s.available),
                    "ready" => format!("nibOS {} is ready to install", s.available),
                    _ => "nibOS updates".into(),
                },
                _ => "nibOS updates".into(),
            }
        };
        let title = match (self.page, cx.game) {
            (Page::Update, _) => update_title(),
            (Page::Achievements, _) => self.ra_title(),
            (Page::Wifi | Page::Bluetooth, _) => radio_title(),
            (Page::Game, Some(game)) => game.name().to_owned(),
            (Page::System, _) if self.clock_view != ClockView::Closed => self.clock_title(),
            (Page::System, _) if self.reading_sys => self.sys.summary(),
            (Page::System, _) => "DSlot and the device".to_owned(),
            _ if cx.console == Console::Apps => "Every app".to_owned(),
            _ => format!("Every {} game", cx.console.label()),
        };
        let items = self.items(cx);
        // Y, where the highlighted row is set here: back to what it inherits.
        let back = items
            .get(self.row)
            .and_then(|item| self.inherits(item, cx))
            .filter(|i| i.set_here)
            .map(|i| format!("Y  use {}   \u{b7}   ", i.from));
        let hint = match (self.page, items.get(self.row)) {
            (Page::Wifi, Some(Item::Info(_))) => {
                "A  check again   \u{b7}   L / R  page   \u{b7}   B  back to Wi-Fi".into()
            }
            (Page::Wifi, Some(Item::Network(name))) if self.known(name) => {
                "A  join or leave   \u{b7}   Y  forget   \u{b7}   L / R  page   \u{b7}   B  back"
                    .into()
            }
            (Page::Wifi, _) => {
                "A  choose   \u{b7}   Y  network info   \u{b7}   L / R  page   \u{b7}   B  back"
                    .into()
            }
            (Page::Bluetooth, Some(Item::Gadget { .. })) => {
                "A  connect, disconnect or pair   \u{b7}   Y  forget   \u{b7}   B  back".into()
            }
            (Page::Bluetooth, _) => "A  choose   \u{b7}   L / R  page   \u{b7}   B  back".into(),
            (Page::Update, _) if self.reading_notes => {
                "A  back to Update   \u{b7}   L / R  page   \u{b7}   B  close".into()
            }
            (Page::System, Some(Item::ClockRow(_))) => {
                "A  choose   \u{b7}   L / R  page   \u{b7}   B  back to System".into()
            }
            (Page::System, Some(Item::ZoneRegion(_))) => "A  choose   \u{b7}   B  back".into(),
            (Page::System, Some(Item::Zone { .. })) => {
                "A  choose   \u{b7}   Left / Right  jump   \u{b7}   B  back".into()
            }
            (Page::System, Some(Item::ClockField(_) | Item::ClockApply)) => {
                "D-pad  change   \u{b7}   A  set the clock   \u{b7}   B  back".into()
            }
            (Page::System, Some(Item::Sys(_))) => "L / R  page   \u{b7}   B  back to System".into(),
            (Page::Update, _) => "A  choose   \u{b7}   L / R  page   \u{b7}   B  back".into(),
            (Page::Achievements, _) if self.ra_signup => {
                "L / R  page   \u{b7}   B  back to Achievements".into()
            }
            (Page::Achievements, Some(Item::Setting(_))) => {
                "D-pad  change   \u{b7}   L / R  page   \u{b7}   B  back".into()
            }
            (Page::Achievements, _) => "A  choose   \u{b7}   L / R  page   \u{b7}   B  back".into(),
            _ => format!(
                "D-pad  change   \u{b7}   {}L / R  page   \u{b7}   B  back",
                back.unwrap_or_default()
            ),
        };
        let row = |(r, item): (usize, &Item)| Row {
            label: item.label().to_owned(),
            value: self.value(item, r, cx),
            source: self.source(item, cx),
        };
        let rows = items.iter().enumerate().map(row).collect();
        Shown {
            tabs,
            title,
            rows,
            scroll: self.scroll,
            row: self.row,
            hint,
            keyboard: self.typing.as_ref().map(|(kb, _)| kb.clone()),
            picture: (self.page == Page::Achievements && self.ra_signup)
                .then(|| achievements::qr(achievements::SIGN_UP))
                .flatten(),
        }
    }

    /// A Network info line's value: a fact, or plainly why there isn't one.
    fn info_value(&self, row: InfoRow) -> Value {
        let net = &self.net;
        let prompt = |text: String, warning: bool| Value::Prompt { text, warning };
        let dash = || prompt("\u{2013}".into(), false);
        if !net.read {
            return dash();
        }
        let off = net.wifi == Some(false);
        let station = net.station.as_ref();
        let joined = station.is_some_and(|s| s.connected());
        // Why there is no address, route or server: the radio, or no network joined yet.
        let because = || {
            prompt(
                if off {
                    "Wi-Fi is off".into()
                } else if !joined {
                    "Not connected".into()
                } else {
                    "None yet".into()
                },
                !off,
            )
        };
        match row {
            InfoRow::Wifi => prompt(
                match (net.wifi, station) {
                    (Some(false), _) => "Off".into(),
                    (_, Some(s)) if s.connected() => "On, connected".into(),
                    (_, Some(s)) if s.state == "connecting" => "On, joining\u{2026}".into(),
                    (Some(true), _) | (None, Some(_)) => "On, not connected".into(),
                    (None, None) => "Unknown".into(),
                },
                false,
            ),
            InfoRow::Network => match station.filter(|s| s.connected()) {
                Some(s) => {
                    let name = s.network.clone().unwrap_or_else(|| "(hidden)".into());
                    let mut text = name;
                    if let Some(dbm) = s.rssi {
                        text.push_str(&format!(", {dbm} dBm"));
                    }
                    if let Some(band) = s.band() {
                        text.push_str(&format!(", {band}"));
                    }
                    // The status bar's bars where it has them, else from the signal read here.
                    let bars = self
                        .radio
                        .wifi
                        .as_ref()
                        .and_then(|w| w.joined.as_ref())
                        .map_or(s.bars(), |(_, bars)| *bars);
                    Value::Status {
                        text,
                        glyphs: vec![nibkit::menu::Glyph::Signal(bars)],
                    }
                }
                None => because(),
            },
            InfoRow::Address => match net.address_line() {
                Some(line) => prompt(line, false),
                None => because(),
            },
            InfoRow::Gateway => match &net.gateway {
                Some(gw) => prompt(gw.clone(), false),
                None => because(),
            },
            // Without a route, resolved lists only its fallback servers: not this network's.
            InfoRow::Dns => match net.dns.as_slice() {
                [] => because(),
                _ if net.gateway.is_none() => because(),
                [a] => prompt(a.clone(), false),
                [a, b] => prompt(format!("{a}, {b}"), false),
                [a, b, rest @ ..] => prompt(format!("{a}, {b} +{}", rest.len()), false),
            },
            InfoRow::Name => prompt(net.name.clone(), false),
            InfoRow::Mac => prompt(net.mac.clone().unwrap_or_else(|| "\u{2013}".into()), false),
            InfoRow::Internet | InfoRow::Updates | InfoRow::Tracker => {
                use netinfo::Reach;
                let (host, reach) = match row {
                    InfoRow::Internet => (None, net.checks.internet),
                    InfoRow::Updates => (Some(netinfo::UPDATE_HOST.to_owned()), net.checks.updates),
                    _ => match &net.tracker {
                        Some(host) => (Some(host.clone()), net.checks.tracker),
                        None => return prompt("Not set".into(), false),
                    },
                };
                let text = match host {
                    Some(host) => format!("{host}: {reach}"),
                    None => reach.to_string(),
                };
                prompt(text, matches!(reach, Reach::NoRoute | Reach::DnsFailed))
            }
        }
    }

    /// A note beside a row's value, where it needs one.
    fn source(&self, item: &Item, _cx: &Context) -> Option<String> {
        match item {
            Item::Control(Control::BatteryProtect) => Some("stops charging at about 80%".into()),
            Item::ImportTuning => {
                Some("from your stock card in TF2 or a stock image in Roms".into())
            }
            Item::SpeakerTuning | Item::RestartNow if self.restart => {
                Some("applies at the next restart".into())
            }
            Item::Rollback => self
                .update
                .as_ref()
                .map(|(s, _)| format!("nibOS {}", s.previous)),
            Item::Sys(row) => self.sys_value(*row).and_then(|(_, note)| note),
            Item::ClockRow(ClockRow::Manual) => {
                Some("network time goes off until turned back on".into())
            }
            Item::ClockRow(ClockRow::Sync) => Some("Wi-Fi comes on if it is off".into()),
            _ => None,
        }
    }

    /// The System info lines the device has a value for, in order.
    fn sys_rows(&self) -> Vec<SysRow> {
        let storage = self.sys.volumes.iter().map(|v| SysRow::Storage(v.name));
        SysRow::FIRST
            .into_iter()
            .chain(storage)
            .chain([SysRow::Battery, SysRow::Display])
            .filter(|row| self.sys_value(*row).is_some())
            .collect()
    }

    /// A System info line's value and the note beside it, or `None` where the device has no
    /// source for it: then the line isn't shown.
    fn sys_value(&self, row: SysRow) -> Option<(Value, Option<String>)> {
        use sysinfo::{mb, of, of_bytes, percent, span};
        let prompt = |text: String| Value::Prompt {
            text,
            warning: false,
        };
        let plain = |text: String| Some((prompt(text), None));
        let sys = &self.sys;
        let memory = sys.memory.as_ref();
        match row {
            SysRow::Board => plain(sys.board.clone()?),
            SysRow::Nibos => {
                let r = sys.release.as_ref()?;
                let mut text = r.version.clone();
                for part in [&r.edition, &r.channel]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                {
                    text.push_str(&format!(" \u{b7} {part}"));
                }
                let note = (!r.build.is_empty()).then(|| format!("build {}", r.build));
                Some((prompt(text), note))
            }
            SysRow::Kernel => plain(sys.kernel.clone()?),
            SysRow::Uptime => plain(span(sys.uptime?)),
            SysRow::Slot => Some((prompt(sys.slot()?.into()), sys.root.clone())),
            SysRow::Dslot => plain(match option_env!("NIBKIT_VERSION") {
                Some(v) => format!("{} \u{b7} nibkit {v}", env!("CARGO_PKG_VERSION")),
                None => env!("CARGO_PKG_VERSION").to_owned(),
            }),
            SysRow::Memory => {
                let m = memory?;
                Some((
                    Value::Slider(percent(m.used(), m.total)),
                    Some(format!("{} in use", of(m.used(), m.total))),
                ))
            }
            SysRow::Available => plain(mb(memory?.available)),
            SysRow::DslotMemory => plain(format!("{} resident", mb(sys.rss?))),
            SysRow::Shared => plain(mb(memory?.shared)),
            SysRow::Cache => plain(mb(memory?.cache)),
            SysRow::Swap => {
                let m = memory?;
                if m.swap_total == 0 {
                    return plain("None".into());
                }
                let on = sys
                    .swap
                    .as_ref()
                    .map_or(String::new(), |dev| format!(" on {dev}"));
                Some((
                    Value::Slider(percent(m.swap_used, m.swap_total)),
                    Some(format!("{} in use{on}", of(m.swap_used, m.swap_total))),
                ))
            }
            SysRow::Cma => {
                let (used, total) = memory?.cma?;
                plain(format!("{} in use", of(used, total)))
            }
            SysRow::Load => {
                let [a, b, c] = sys.load?;
                Some((
                    prompt(format!("{a:.2} \u{b7} {b:.2} \u{b7} {c:.2}")),
                    Some("1, 5, 15 min".into()),
                ))
            }
            SysRow::Cpu => {
                let cores = &sys.cores;
                let first = *cores.first()?;
                let text = if cores.iter().all(|&c| c == first) {
                    format!("{} \u{d7} {} MHz", cores.len(), first / 1000)
                } else {
                    let each: Vec<String> = cores.iter().map(|c| (c / 1000).to_string()).collect();
                    format!("{} MHz", each.join(" \u{b7} "))
                };
                Some((prompt(text), sys.governor.clone()))
            }
            SysRow::Temperature => {
                if sys.zones.is_empty() {
                    return None;
                }
                let each: Vec<String> = sys
                    .zones
                    .iter()
                    .map(|z| format!("{} {} \u{b0}C", z.name.to_uppercase(), z.millideg / 1000))
                    .collect();
                plain(each.join(" \u{b7} "))
            }
            SysRow::Gpu => {
                let (hz, governor) = sys.gpu.as_ref()?;
                let note = (!governor.is_empty()).then(|| governor.clone());
                Some((prompt(format!("{} MHz", hz / 1_000_000)), note))
            }
            SysRow::Storage(name) => {
                let v = sys.volumes.iter().find(|v| v.name == name)?;
                Some((
                    Value::Slider(percent(v.used, v.total)),
                    Some(format!("{} used", of_bytes(v.used, v.total))),
                ))
            }
            SysRow::Battery => plain(self.battery.as_ref()?.0.clone()),
            SysRow::Display => {
                if sys.panels.is_empty() {
                    return None;
                }
                let each: Vec<String> = sys
                    .panels
                    .iter()
                    .map(|(connector, mode)| format!("{connector} {}", mode.replace('x', "\u{d7}")))
                    .collect();
                plain(each.join(" \u{b7} "))
            }
        }
    }

    fn value(&self, item: &Item, r: usize, cx: &Context) -> Value {
        let text = |s: &str| Value::Text(s.to_owned());
        if let Some((row, note)) = &self.note {
            if *row == r {
                return Value::Text(note.clone());
            }
        }
        match item {
            Item::Favourite => text(match cx.game {
                Some(g) if cx.stats.fave(&g.path) => "Yes",
                Some(_) => "No",
                None => "\u{2013}",
            }),
            Item::Rating => Value::Stars(cx.game.map_or(0, |g| cx.stats.rating(&g.path))),
            // Not set here: what it goes back to, muted.
            Item::Setting(_) | Item::Emulator
                if self.inherits(item, cx).is_some_and(|i| !i.set_here) =>
            {
                Value::Inherited(
                    self.inherits(item, cx)
                        .map(|i| i.text())
                        .unwrap_or_default(),
                )
            }
            Item::Setting(def) => match cx.settings.value(&cx.chain(self.page), def) {
                Some(v) => Value::Text(def.show(&v)),
                None => text("\u{2013}"),
            },
            Item::Emulator => text(
                cx.emulator(self.page)
                    .map_or("\u{2013}", |e| e.name.as_str()),
            ),
            Item::Control(c) => match self
                .controls
                .iter()
                .find(|(k, _)| k == c)
                .and_then(|(_, v)| *v)
            {
                Some(v) if c.is_switch() => text(if v != 0 { "On" } else { "Off" }),
                Some(v) => Value::Slider(v),
                None => text("\u{2013}"),
            },
            Item::Clock => text(if self.hour12 { "12-hour" } else { "24-hour" }),
            Item::DateTime
            | Item::ClockRow(_)
            | Item::ZoneRegion(_)
            | Item::Zone { .. }
            | Item::ClockField(_)
            | Item::ClockApply => self.clock_value(item, r),
            Item::Joined => {
                let wifi = self.radio.wifi.as_ref();
                match wifi.and_then(|w| w.joined.as_ref()) {
                    // The network joined, with its signal.
                    Some((name, bars)) => Value::Status {
                        text: name.clone(),
                        glyphs: vec![nibkit::menu::Glyph::Signal(*bars)],
                    },
                    None => Value::Prompt {
                        text: wifi.map_or("\u{2013}".into(), |w| w.state.clone()),
                        warning: false,
                    },
                }
            }
            Item::Address => Value::Prompt {
                text: match (&self.net.address, self.net.read) {
                    (Some((ip, _)), _) => ip.clone(),
                    (None, true)
                        if self.radio.wifi.as_ref().is_some_and(|w| w.joined.is_some()) =>
                    {
                        "No address yet".into()
                    }
                    (None, true) => "No address".into(),
                    (None, false) => "\u{2013}".into(),
                },
                warning: false,
            },
            Item::NetworkInfo => Value::Prompt {
                text: if r == self.row {
                    "Press A".into()
                } else {
                    String::new()
                },
                warning: false,
            },
            Item::Info(row) => self.info_value(*row),
            Item::SystemInfo => Value::Prompt {
                text: if r == self.row {
                    "Press A".into()
                } else {
                    String::new()
                },
                warning: false,
            },
            Item::Sys(row) => self
                .sys_value(*row)
                .map_or_else(|| text("\u{2013}"), |(v, _)| v),
            Item::Scan => Value::Prompt {
                text: if self.radio.wifi.as_ref().is_some_and(|w| w.scanning) {
                    "Looking\u{2026}".into()
                } else if r == self.row {
                    "Press A".into()
                } else {
                    String::new()
                },
                warning: false,
            },
            Item::Network(name) => {
                let net = self
                    .radio
                    .wifi
                    .as_ref()
                    .and_then(|w| w.networks.iter().find(|n| &n.name == name));
                let Some(n) = net else {
                    return text("\u{2013}");
                };
                if self.forget == Some(r) {
                    return Value::Prompt {
                        text: "Press Y again to forget it".into(),
                        warning: true,
                    };
                }
                if self.armed == Some(r) {
                    return Value::Prompt {
                        text: "Press A again to leave it".into(),
                        warning: true,
                    };
                }
                let what = match (n.connected, n.known, n.locked, n.unsupported) {
                    (true, ..) => "Joined",
                    (_, true, ..) => "Saved",
                    (.., true) => "Not supported",
                    (_, _, true, _) => "Passphrase",
                    _ => "Open",
                };
                use nibkit::menu::Glyph;
                let mut glyphs = Vec::new();
                if n.connected {
                    glyphs.push(Glyph::Check);
                }
                if n.secured {
                    glyphs.push(Glyph::Lock);
                }
                glyphs.push(Glyph::Signal(n.bars));
                Value::Status {
                    text: what.into(),
                    glyphs,
                }
            }
            Item::BluetoothPower => text(match &self.radio.bluetooth {
                Some(b) if b.powered => "On",
                Some(_) => "Off",
                None => "\u{2013}",
            }),
            Item::PairNew => Value::Prompt {
                text: if self.radio.bluetooth.as_ref().is_some_and(|b| b.discovering) {
                    "Looking: put the device in pairing mode".into()
                } else if r == self.row {
                    "Press A to look".into()
                } else {
                    String::new()
                },
                warning: false,
            },
            Item::Gadget { address, .. } => {
                let g = self
                    .radio
                    .bluetooth
                    .as_ref()
                    .and_then(|b| b.devices.iter().find(|g| &g.address == address));
                let Some(g) = g else { return text("\u{2013}") };
                if self.forget == Some(r) {
                    return Value::Prompt {
                        text: "Press Y again to forget it".into(),
                        warning: true,
                    };
                }
                // As every app's Connections page shows it.
                match super::radios::device_value(g) {
                    nibkit::menu::Value::Status { text, glyphs } => Value::Status { text, glyphs },
                    _ => text("\u{2013}"),
                }
            }
            Item::UpdateVersion => Value::Prompt {
                text: match self.update.as_ref().map(|(s, _)| s) {
                    Some(s) if !s.version.is_empty() && !s.edition.is_empty() => {
                        format!("{} ({})", s.version, s.edition)
                    }
                    Some(s) if !s.version.is_empty() => s.version.clone(),
                    _ => "\u{2013}".into(),
                },
                warning: false,
            },
            Item::UpdateChannel => text(match self.update.as_ref().map(|(s, _)| s) {
                Some(s) if s.channel == "beta" => "Beta",
                Some(s) if s.channel == "stable" => "Stable",
                _ => "\u{2013}",
            }),
            Item::UpdateState => self.update_value(r),
            Item::WhatsNew => Value::Prompt {
                text: if r == self.row {
                    "Press A".into()
                } else {
                    String::new()
                },
                warning: false,
            },
            Item::NoteLine(_) | Item::Message(_) => Value::Prompt {
                text: String::new(),
                warning: false,
            },
            Item::RaUser
            | Item::RaSignIn
            | Item::RaAccount
            | Item::RaSignOut
            | Item::RaSignUp
            | Item::RaLink => self.ra_value(item, r),
            Item::SpeakerTuning => text(match self.tuning {
                Some(nibkit::audio::Tuning::Nibos) => "nibOS",
                Some(nibkit::audio::Tuning::Official) => "Official",
                Some(nibkit::audio::Tuning::Other) => "Other",
                Some(nibkit::audio::Tuning::None) => "None",
                None => "\u{2013}",
            }),
            Item::Battery => Value::Prompt {
                text: self
                    .battery
                    .as_ref()
                    .map_or("\u{2013}".into(), |(l, _)| l.clone()),
                warning: false,
            },
            Item::PowerOnStart => {
                text(
                    self.at_boot
                        .map_or("\u{2013}", |on| if on { "On" } else { "Off" }),
                )
            }
            Item::Action(_)
            | Item::DeleteSaves
            | Item::PowerOff
            | Item::ImportTuning
            | Item::RestartNow
            | Item::Rollback
            | Item::Rescan => {
                let what = match item {
                    Item::PowerOff => "power off",
                    Item::RestartNow => "restart",
                    Item::Rollback => "restart into it",
                    Item::ImportTuning => "import",
                    _ => "do it",
                };
                match (self.armed == Some(r), r == self.row) {
                    (true, _) => Value::Prompt {
                        text: format!("Press A again to {what}"),
                        warning: true,
                    },
                    (false, true) => Value::Prompt {
                        text: "Press A".into(),
                        warning: false,
                    },
                    (false, false) => Value::Prompt {
                        text: String::new(),
                        warning: false,
                    },
                }
            }
        }
    }
}

impl Menu {
    /// The Update row's value: what A does there, in a word or two (the title line says how
    /// things stand), or the download's progress.
    fn update_value(&self, r: usize) -> Value {
        let prompt = |text: &str, warning: bool| Value::Prompt {
            text: text.into(),
            warning,
        };
        let picked = r == self.row;
        let Some((s, _)) = &self.update else {
            return prompt("\u{2013}", false);
        };
        match s.shown_phase() {
            "checking" => prompt("Checking\u{2026}", false),
            "current" if picked => prompt("Press A to check again", false),
            "current" => prompt("Up to date", false),
            "available" => prompt(&format!("Download ({} MB)", s.megabytes()), false),
            "downloading" => Value::Slider(i64::from(s.percent())),
            "paused" => prompt(&format!("Resume ({}% done)", s.percent()), false),
            "finishing" => prompt("Finishing\u{2026}", false),
            "ready" if self.armed == Some(r) => prompt("Press A again to restart", true),
            "ready" => prompt("Restart to update", false),
            "restarting" => prompt("Restarting\u{2026}", false),
            "trying" => prompt("Starting\u{2026}", false),
            "error" => prompt("Try again", true),
            _ if picked => prompt("Press A to check", false),
            _ => prompt("Not checked", false),
        }
    }
}

/// Where the row `place` rows below the first on screen sits: its middle.
/// Whether `def` is an emulator's option, rather than one of DSlot's own settings.
fn is_option(def: &Def) -> bool {
    def::builtin(&def.key).is_none()
}

/// The Date and time page (clock.rs).
impl Menu {
    /// Its rows, as the view stands.
    fn clock_items(&self, items: &mut Vec<Item>) {
        match &self.clock_view {
            ClockView::Closed => {}
            ClockView::Page => items.extend(ClockRow::ALL.into_iter().map(Item::ClockRow)),
            ClockView::Regions => {
                items.push(Item::ZoneRegion("UTC".into()));
                if let Some(zones) = &self.zones {
                    items.extend(
                        zones
                            .regions
                            .iter()
                            .map(|(r, _)| Item::ZoneRegion(r.clone())),
                    );
                }
            }
            ClockView::Places(region) => {
                let places = self.zones.as_ref().map_or(&[][..], |z| z.of(region));
                items.extend(places.iter().map(|id| Item::Zone {
                    id: id.clone(),
                    label: clock::place(id),
                }));
                if items.is_empty() {
                    items.push(Item::ZoneRegion(region.clone()));
                }
            }
            ClockView::Manual => {
                items.extend(clock::Field::ALL.into_iter().map(Item::ClockField));
                items.push(Item::ClockApply);
            }
        }
    }

    /// Opens the page over the System page's rows.
    fn open_clock(&mut self, _cx: &Context) {
        self.clock_view = ClockView::Page;
        (self.row, self.scroll, self.armed, self.note) = (0, 0, None, None);
    }

    /// B: back a level; from the page itself, back to the System page on its row.
    fn clock_back(&mut self, cx: &Context) {
        let (view, row) = match &self.clock_view {
            ClockView::Closed | ClockView::Page => (ClockView::Closed, None),
            ClockView::Regions => (ClockView::Page, Some(ClockRow::Zone)),
            ClockView::Places(_) => (ClockView::Regions, None),
            ClockView::Manual => (ClockView::Page, Some(ClockRow::Manual)),
        };
        // Back in the regions, on the one left.
        let region = match &self.clock_view {
            ClockView::Places(r) => Some(r.clone()),
            _ => None,
        };
        self.clock_view = view;
        (self.armed, self.note) = (None, None);
        let items = self.items(cx);
        self.row = match (row, region) {
            (Some(r), _) => items.iter().position(|i| *i == Item::ClockRow(r)),
            (None, Some(region)) => items
                .iter()
                .position(|i| *i == Item::ZoneRegion(region.clone())),
            (None, None) if self.clock_view == ClockView::Closed => {
                items.iter().position(|i| *i == Item::DateTime)
            }
            _ => Some(0),
        }
        .unwrap_or(0)
        .min(items.len().saturating_sub(1));
        self.scroll = self.row.saturating_sub(VISIBLE - 1);
    }

    /// A button on a row of the page: `chosen` A (or a tap), `turn` left or right.
    fn clock_row(&mut self, item: &Item, chosen: bool, turn: i32, cx: &Context) {
        match (item, &self.clock_view) {
            (Item::ClockRow(ClockRow::Zone), _) if chosen => {
                if self.zones.is_none() {
                    self.zones = Some(clock::zones());
                }
                self.clock_view = ClockView::Regions;
                (self.row, self.scroll, self.note) = (0, 0, None);
                // On the region of the zone set, where it is one of them.
                let region = self.time.zone.split_once('/').map(|(r, _)| r.to_owned());
                let items = self.items(cx);
                if let Some(r) = region.and_then(|r| {
                    items
                        .iter()
                        .position(|i| matches!(i, Item::ZoneRegion(x) if *x == r))
                }) {
                    self.row = r;
                    self.scroll = r.saturating_sub(VISIBLE - 1);
                }
            }
            (Item::ClockRow(ClockRow::Ntp), _) if chosen || turn != 0 => {
                let on = self.time.ntp.unwrap_or(false);
                self.clock_do(ClockRow::Ntp, clock::Do::Ntp(!on));
            }
            (Item::ClockRow(ClockRow::Sync), _) if chosen => {
                self.clock_do(ClockRow::Sync, clock::Do::Sync)
            }
            (Item::ClockRow(ClockRow::Manual), _) if chosen => {
                self.draft = clock::Draft::from(self.time.local);
                self.clock_view = ClockView::Manual;
                (self.row, self.scroll, self.note) = (0, 0, None);
            }
            (Item::ZoneRegion(region), ClockView::Regions) if chosen => {
                if region == "UTC" {
                    self.clock_view = ClockView::Page;
                    self.clock_do_on(ClockRow::Zone, clock::Do::Zone("UTC".into()), cx);
                } else {
                    self.clock_view = ClockView::Places(region.clone());
                    (self.row, self.scroll) = (0, 0);
                    let items = self.items(cx);
                    let current = items
                        .iter()
                        .position(|i| matches!(i, Item::Zone { id, .. } if *id == self.time.zone));
                    if let Some(r) = current {
                        self.row = r;
                        self.scroll = r.saturating_sub(VISIBLE - 1);
                    }
                }
            }
            (Item::Zone { id, .. }, ClockView::Places(_)) if chosen => {
                let id = id.clone();
                self.clock_view = ClockView::Page;
                self.clock_do_on(ClockRow::Zone, clock::Do::Zone(id), cx);
            }
            // Left and right jump through a long list of places.
            (Item::Zone { .. }, ClockView::Places(_)) if turn != 0 => {
                let n = self.items(cx).len() as i32;
                self.row =
                    (self.row as i32 + turn.signum() * VISIBLE as i32).clamp(0, n - 1) as usize;
                self.scroll = self
                    .scroll
                    .clamp(self.row.saturating_sub(VISIBLE - 1), self.row);
            }
            (Item::ClockField(field), ClockView::Manual) if turn != 0 => {
                self.draft.step(*field, turn.signum())
            }
            (Item::ClockApply, ClockView::Manual) if chosen => {
                let at = self.draft.argument();
                self.clock_view = ClockView::Page;
                self.clock_do_on(ClockRow::Manual, clock::Do::Set(at), cx);
            }
            _ => {}
        }
    }

    /// Runs a command for the row picked, which says "Working…" until it answers. One at a
    /// time.
    fn clock_do(&mut self, row: ClockRow, what: clock::Do) {
        if self.clock_job.is_some() {
            return;
        }
        self.note = Some((self.row, "Working\u{2026}".into()));
        self.clock_job = Some((row, clock::run(what)));
    }

    /// The same, back on the page with `row` picked (after the picker or the fields).
    fn clock_do_on(&mut self, row: ClockRow, what: clock::Do, cx: &Context) {
        let items = self.items(cx);
        self.row = items
            .iter()
            .position(|i| *i == Item::ClockRow(row))
            .unwrap_or(0);
        self.scroll = self.row.saturating_sub(VISIBLE - 1);
        self.clock_do(row, what);
    }

    /// The running command's outcome, when it has come: on its row, and the status read again.
    fn clock_job_done(&mut self, cx: &Context) {
        let Some((row, rx)) = &self.clock_job else {
            return;
        };
        let Ok(outcome) = rx.try_recv() else { return };
        let row = *row;
        self.clock_job = None;
        if let Some(watch) = &self.clock {
            watch.poke();
        }
        if self.clock_view == ClockView::Page {
            let at = self
                .items(cx)
                .iter()
                .position(|i| *i == Item::ClockRow(row));
            self.note = at.map(|r| (r, outcome.to_string()));
        }
    }

    /// The title line: where the time came from, or what is being picked.
    fn clock_title(&self) -> String {
        match &self.clock_view {
            ClockView::Closed => String::new(),
            ClockView::Page if !self.time.read => "The clock".into(),
            ClockView::Page => self.time.source(self.hour12).0,
            ClockView::Regions => "Time zone: choose a region".into(),
            ClockView::Places(region) => format!("Time zone: choose a place in {region}"),
            ClockView::Manual => "Set the date and time: network time goes off".into(),
        }
    }

    /// A row's value on the page, the picker or the fields.
    fn clock_value(&self, item: &Item, r: usize) -> Value {
        let prompt = |text: String, warning: bool| Value::Prompt { text, warning };
        let press = |here: bool| {
            prompt(
                if here {
                    "Press A".into()
                } else {
                    String::new()
                },
                false,
            )
        };
        let dash = "\u{2013}".to_owned();
        match item {
            Item::DateTime => prompt(self.time.glance(self.hour12).unwrap_or_default(), false),
            Item::ClockRow(ClockRow::Time) => {
                prompt(self.time.local.map_or(dash, |t| t.time(self.hour12)), false)
            }
            Item::ClockRow(ClockRow::Date) => prompt(self.time.date_line().unwrap_or(dash), false),
            Item::ClockRow(ClockRow::Zone) => prompt(self.time.zone_line(), false),
            Item::ClockRow(ClockRow::Source) => {
                let (text, warning) = self.time.source(self.hour12);
                prompt(text, warning)
            }
            Item::ClockRow(ClockRow::Ntp) => Value::Text(
                match self.time.ntp {
                    Some(true) => "On",
                    Some(false) => "Off",
                    None => "\u{2013}",
                }
                .into(),
            ),
            Item::ClockRow(ClockRow::Sync | ClockRow::Manual) | Item::ClockApply => {
                press(r == self.row)
            }
            Item::ZoneRegion(region) => {
                let here =
                    self.time.zone == *region || self.time.zone.starts_with(&format!("{region}/"));
                if here {
                    prompt("Current".into(), false)
                } else {
                    press(r == self.row)
                }
            }
            Item::Zone { id, .. } if *id == self.time.zone => Value::Status {
                text: "Current".into(),
                glyphs: vec![nibkit::menu::Glyph::Check],
            },
            Item::Zone { .. } => press(r == self.row),
            Item::ClockField(field) => Value::Text(self.draft.show(*field, self.hour12)),
            _ => prompt(String::new(), false),
        }
    }
}

/// The Achievements page (achievements.rs).
impl Menu {
    /// Its rows: signed out, the username, Sign in, Mode and Make an account; signed in, who,
    /// Sign out and Mode. The sign-up view has the address alone, over the QR code.
    fn ra_items(&self, items: &mut Vec<Item>) {
        if self.ra_signup {
            items.push(Item::RaLink);
            return;
        }
        let mode = def::builtin(def::ACHIEVEMENTS).map(Item::Setting);
        match &self.ra_account {
            Some(_) => {
                items.extend([Item::RaAccount, Item::RaSignOut]);
                items.extend(mode);
            }
            None => {
                items.extend([Item::RaUser, Item::RaSignIn]);
                items.extend(mode);
                items.push(Item::RaSignUp);
            }
        }
    }

    /// A button on a row of the page: `chosen` A (or a tap).
    fn ra_row(&mut self, item: &Item, chosen: bool) {
        if !chosen {
            return;
        }
        match item {
            Item::RaUser => {
                let mut kb = Keyboard::new("RetroAchievements username", Purpose::Text, 20);
                for c in self.ra_user.chars() {
                    kb.type_char(c);
                }
                self.typing = Some((kb, Typing::RaUser));
            }
            Item::RaSignIn if self.ra_job.is_some() => {}
            Item::RaSignIn if self.ra_user.is_empty() => {
                self.note = Some((self.row, "Type the username first".into()));
            }
            Item::RaSignIn => {
                let user = self.ra_user.clone();
                let kb = Keyboard::new(format!("Password for {user}"), Purpose::Secret, 64);
                self.typing = Some((kb, Typing::RaPassword(user)));
            }
            Item::RaSignOut if self.armed == Some(self.row) => {
                self.armed = None;
                if let Err(e) = Account::clear(&self.ra_path) {
                    eprintln!("dslot: cannot remove {}: {e}", self.ra_path.display());
                }
                eprintln!("dslot: RetroAchievements: signed out");
                (self.ra_account, self.ra_standing, self.ra_said) = (None, None, None);
                (self.row, self.scroll) = (0, 0);
            }
            Item::RaSignOut => self.armed = Some(self.row),
            Item::RaSignUp => {
                self.ra_signup = true;
                (self.row, self.scroll, self.armed, self.note) = (0, 0, None, None);
            }
            _ => {}
        }
    }

    /// B from the sign-up view: back to the page, on Make an account.
    fn ra_back(&mut self, cx: &Context) {
        self.ra_signup = false;
        let items = self.items(cx);
        self.row = items.iter().position(|i| *i == Item::RaSignUp).unwrap_or(0);
        self.scroll = self.row.saturating_sub(VISIBLE - 1);
    }

    /// Starts a request for `item`'s row, which says "Signing in…" until it answers. One at
    /// a time.
    fn ra_go(&mut self, item: Item, rx: std::sync::mpsc::Receiver<Outcome>) {
        if self.ra_job.is_some() {
            return;
        }
        self.note = Some((self.row, "Signing in\u{2026}".into()));
        self.ra_said = None;
        self.ra_job = Some((item, rx));
    }

    /// Checks the sign-in kept with the service, once per opening of the menu: whether the
    /// token is still good, and the player's points now.
    fn ra_check(&mut self) {
        if self.ra_checked || self.ra_job.is_some() {
            return;
        }
        self.ra_checked = true;
        if let Some(account) = &self.ra_account {
            self.ra_job = Some((Item::RaAccount, achievements::check(account)));
        }
    }

    /// The request's outcome, when it has come: a sign-in kept, a refusal or a failure said.
    fn ra_job_done(&mut self, cx: &Context) {
        let Some((item, rx)) = &self.ra_job else {
            return;
        };
        let Ok(outcome) = rx.try_recv() else { return };
        let item = item.clone();
        self.ra_job = None;
        let line = match &outcome {
            Outcome::SignedIn(standing, token) => {
                let account = Account {
                    user: standing.user.clone(),
                    token: token.clone(),
                };
                if item == Item::RaSignIn {
                    match account.save(&self.ra_path) {
                        Ok(()) => {
                            eprintln!("dslot: RetroAchievements: signed in as {}", account.user)
                        }
                        Err(e) => eprintln!("dslot: cannot write {}: {e}", self.ra_path.display()),
                    }
                }
                self.ra_user = account.user.clone();
                self.ra_account = Some(account);
                self.ra_standing = Some(standing.clone());
                None
            }
            Outcome::Refused(why) if item == Item::RaAccount => {
                // The token no longer stands (the password changed, say): sign in again.
                eprintln!("dslot: RetroAchievements: the sign-in kept was refused: {why}");
                if let Err(e) = Account::clear(&self.ra_path) {
                    eprintln!("dslot: cannot remove {}: {e}", self.ra_path.display());
                }
                (self.ra_account, self.ra_standing) = (None, None);
                Some("The sign-in kept is no longer good: sign in again".to_owned())
            }
            Outcome::Refused(why) => {
                eprintln!("dslot: RetroAchievements: {why}");
                Some(why.clone())
            }
            // Offline: the sign-in kept stands as it is.
            Outcome::NoNetwork if item == Item::RaAccount => None,
            other => {
                eprintln!("dslot: RetroAchievements: {}", other.text());
                Some(other.text())
            }
        };
        self.ra_said = line.clone();
        if self.page == Page::Achievements && !self.ra_signup {
            let items = self.items(cx);
            // Signed in: on the account row; refused: back on Sign in.
            let wanted = match &self.ra_account {
                Some(_) => Item::RaAccount,
                None => Item::RaSignIn,
            };
            if let Some(r) = items.iter().position(|i| *i == wanted) {
                (self.row, self.scroll) = (r, r.saturating_sub(VISIBLE - 1));
                self.note = line.map(|l| (r, l));
            }
        }
    }

    /// The title line: who is signed in, or what the service said.
    fn ra_title(&self) -> String {
        if self.ra_signup {
            return "Make an account on a phone: scan the code".into();
        }
        match (&self.ra_said, &self.ra_account, &self.ra_job) {
            (Some(said), ..) => said.clone(),
            (None, _, Some((Item::RaSignIn, _))) => {
                "Signing in to RetroAchievements\u{2026}".into()
            }
            (None, Some(a), _) => format!("RetroAchievements: signed in as {}", a.user),
            (None, None, _) => "RetroAchievements: not signed in".into(),
        }
    }

    /// A row's value on the page.
    fn ra_value(&self, item: &Item, r: usize) -> Value {
        let prompt = |text: String, warning: bool| Value::Prompt { text, warning };
        let press =
            |here: bool, what: &str| prompt(if here { what.into() } else { String::new() }, false);
        match item {
            Item::RaUser if self.ra_user.is_empty() => press(r == self.row, "Press A to type it"),
            Item::RaUser => Value::Text(self.ra_user.clone()),
            Item::RaSignIn if self.ra_job.is_some() => prompt("Signing in\u{2026}".into(), false),
            Item::RaSignIn => press(r == self.row, "Press A for the password"),
            Item::RaAccount => {
                let user = self
                    .ra_account
                    .as_ref()
                    .map_or("\u{2013}", |a| a.user.as_str());
                match &self.ra_standing {
                    Some(s) => Value::Status {
                        text: format!("{user} \u{b7} {}", s.points()),
                        glyphs: vec![nibkit::menu::Glyph::Check],
                    },
                    None if self.ra_job.is_some() => {
                        prompt(format!("{user} \u{b7} checking\u{2026}"), false)
                    }
                    None => prompt(user.to_owned(), false),
                }
            }
            Item::RaSignOut if self.armed == Some(r) => {
                prompt("Press A again to sign out".into(), true)
            }
            Item::RaSignOut => press(r == self.row, "Press A"),
            Item::RaSignUp => press(r == self.row, "Press A"),
            Item::RaLink => prompt(
                achievements::SIGN_UP
                    .trim_start_matches("https://")
                    .to_owned(),
                false,
            ),
            _ => prompt(String::new(), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Legacy;
    use crate::testing::TempDir;
    use std::path::PathBuf;

    const DSPERATE: &str = r#"
        name = "DSperate"
        priority = 100
        [systems]
        NDS = ["/bin/sh", "{options}", "{rom}"]
        GBA = ["/bin/sh", "--gba", "{options}", "{rom}"]
        [[options]]
        key = "hires3d"
        label = "3D resolution"
        systems = ["NDS"]
        values = [1, 2, 4]
        labels = ["1x", "2x", "4x"]
        default = 2
        args = ["--hires3d", "{value}"]
        [[actions]]
        label = "Clear resume state"
        delete = ["{saves}/*.auto.dss"]
    "#;
    const OTHER: &str = "name = \"Other\"\n[systems]\nGBA = [\"/bin/sh\", \"{rom}\"]\n";

    struct World {
        _dir: TempDir,
        emulators: Emulators,
        settings: Settings,
        stats: Stats,
        game: Game,
    }

    fn world(name: &str, other: bool) -> World {
        let dir = TempDir::new(name);
        let emus = dir.path().join("emulators");
        std::fs::create_dir_all(&emus).unwrap();
        std::fs::write(emus.join("dsperate.toml"), DSPERATE).unwrap();
        if other {
            std::fs::write(emus.join("other.toml"), OTHER).unwrap();
        }
        let emulators = Emulators::load(&[emus]);
        let settings = Settings::load(
            dir.path().join("settings.toml"),
            &Legacy::none(),
            &[],
            &emulators,
        );
        let stats = Stats::load(dir.path().join("stats.tsv"));
        let path = PathBuf::from("/r/NDS/Golden Sun.nds");
        let stem = "Golden Sun".to_owned();
        let game = Game {
            console: Console::Ds,
            path,
            title: vec![stem.clone()],
            stem,
            code: String::new(),
            icon: None,
            cover: None,
            gone: false,
        };
        World {
            _dir: dir,
            emulators,
            settings,
            stats,
            game,
        }
    }

    /// `pages`, and the Wi-Fi and Bluetooth pages where the machine running the tests has iwd
    /// or BlueZ to talk to (a Linux build machine may), and Achievements where it has curl.
    fn with_radios(menu: &Menu, mut pages: Vec<Page>) -> Vec<Page> {
        if menu.radios.is_some() {
            pages.extend([Page::Wifi, Page::Bluetooth]);
        }
        if achievements::available() {
            pages.push(Page::Achievements);
        }
        pages
    }

    /// The Achievements page: signed out, a sign-in (the service's answer injected), the
    /// token kept for the emulators, signing out, and the sign-up view.
    #[test]
    fn achievements_page() {
        let mut w = world("menu-achievements", false);
        let mut cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: None,
        };
        let dir = TempDir::new("menu-achievements-token");
        let token = dir.path().join("retroachievements.token");
        let mut menu = Menu::open(false);
        menu.ra_path = token.clone();
        (menu.ra_account, menu.ra_user) = (None, String::new());
        menu.page = Page::Achievements;
        assert_eq!(
            labels(&menu, &cx),
            ["Username", "Sign in", "Mode", "Make an account"]
        );
        let shown = menu.shown(&cx);
        assert_eq!(shown.title, "RetroAchievements: not signed in");
        assert_eq!(shown.rows[2].value, Value::Text("Softcore".into()));
        assert!(shown.picture.is_none());
        let press = Input {
            pressed: true,
            ..Input::default()
        };
        // Sign in with no username: told so.
        menu.row = 1;
        menu.update(&press, &mut cx);
        assert_eq!(
            menu.shown(&cx).rows[1].value,
            Value::Text("Type the username first".into())
        );
        // A on Username opens the keyboard; what is typed is kept on the row.
        menu.row = 0;
        menu.update(&press, &mut cx);
        assert!(menu.typing());
        menu.typing = None;
        menu.ra_user = "alan".into();
        assert_eq!(menu.shown(&cx).rows[0].value, Value::Text("alan".into()));
        // A on Sign in asks for the password; the request is then on its way.
        menu.row = 1;
        menu.update(&press, &mut cx);
        assert!(matches!(
            menu.typing.as_ref().map(|(_, t)| t),
            Some(Typing::RaPassword(u)) if u == "alan"
        ));
        menu.typing = None;
        let (tx, rx) = std::sync::mpsc::channel();
        menu.ra_go(Item::RaSignIn, rx);
        assert_eq!(
            menu.shown(&cx).rows[1].value,
            Value::Text("Signing in\u{2026}".into())
        );
        // The service answers: signed in, the token kept, the rows change.
        let standing = Standing {
            user: "Alan".into(),
            score: 1200,
            softcore: 0,
            messages: 0,
        };
        tx.send(Outcome::SignedIn(standing, "t0k3n".into()))
            .unwrap();
        menu.update(&Input::default(), &mut cx);
        assert_eq!(labels(&menu, &cx), ["Signed in as", "Sign out", "Mode"]);
        assert_eq!(
            menu.shown(&cx).title,
            "RetroAchievements: signed in as Alan"
        );
        assert_eq!(
            menu.shown(&cx).rows[0].value,
            Value::Status {
                text: "Alan \u{b7} 1,200 points".into(),
                glyphs: vec![nibkit::menu::Glyph::Check]
            }
        );
        assert_eq!(std::fs::read_to_string(&token).unwrap(), "Alan\nt0k3n\n");
        assert_eq!(
            achievements::env(&token, cx.settings.hardcore()),
            [
                ("RA_TOKEN_FILE".to_owned(), token.display().to_string()),
                ("RA_USERNAME".to_owned(), "Alan".to_owned()),
                ("RA_HARDCORE".to_owned(), "0".to_owned()),
            ]
        );
        // Mode steps to hardcore, a System setting.
        menu.row = 2;
        let turn = Input {
            turn: 1,
            ..Input::default()
        };
        assert!(menu.update(&turn, &mut cx).changed);
        assert!(cx.settings.hardcore());
        assert_eq!(achievements::env(&token, true)[2].1, "1");
        // Sign out asks, then removes the token; the username stays typed.
        menu.row = 1;
        menu.update(&press, &mut cx);
        assert_eq!(
            menu.shown(&cx).rows[1].value,
            Value::Prompt {
                text: "Press A again to sign out".into(),
                warning: true
            }
        );
        menu.update(&press, &mut cx);
        assert_eq!(
            labels(&menu, &cx),
            ["Username", "Sign in", "Mode", "Make an account"]
        );
        assert!(!token.exists());
        assert_eq!(menu.shown(&cx).rows[0].value, Value::Text("Alan".into()));
        // A refusal on sign-in stays on the Sign in row, with the reason.
        let (tx, rx) = std::sync::mpsc::channel();
        menu.row = 1;
        menu.ra_go(Item::RaSignIn, rx);
        tx.send(Outcome::Refused(
            "Invalid user/password combination.".into(),
        ))
        .unwrap();
        menu.update(&Input::default(), &mut cx);
        assert_eq!(menu.items(&cx)[menu.row], Item::RaSignIn);
        assert_eq!(
            menu.shown(&cx).rows[1].value,
            Value::Text("Invalid user/password combination.".into())
        );
        assert_eq!(menu.shown(&cx).title, "Invalid user/password combination.");
        // Make an account: the address and the code; B comes back to its row.
        menu.row = 3;
        menu.update(&press, &mut cx);
        assert_eq!(labels(&menu, &cx), ["On a phone"]);
        let shown = menu.shown(&cx);
        assert_eq!(
            shown.rows[0].value,
            Value::Prompt {
                text: "retroachievements.org/createaccount.php".into(),
                warning: false
            }
        );
        assert!(shown.picture.is_some());
        assert!(shown.hint.contains("back to Achievements"));
        let close = Input {
            close: true,
            ..Input::default()
        };
        assert!(!menu.update(&close, &mut cx).closed);
        assert_eq!(menu.items(&cx)[menu.row], Item::RaSignUp);
    }

    fn labels(menu: &Menu, cx: &Context) -> Vec<String> {
        menu.items(cx)
            .iter()
            .map(|i| i.label().to_owned())
            .collect()
    }

    fn moved(n: i32) -> Input {
        Input {
            moved: n,
            ..Input::default()
        }
    }

    #[test]
    fn pages_and_rows() {
        let mut w = world("menu-pages", false);
        let game = w.game.clone();
        let cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: Some(&game),
        };
        let mut menu = Menu::open(true);
        // One emulator: no Emulator row. Its option for DS; its action and the saves' deletion
        // where there is a saves folder of DSlot's (nibOS, not the stock OS).
        let mut game_rows = vec!["Favourite", "Rating", "3D resolution"];
        if platform::saves().is_some() {
            game_rows.extend(["Clear resume state", "Delete saves"]);
        }
        assert_eq!(labels(&menu, &cx), game_rows);
        assert_eq!(
            menu.pages(&cx),
            with_radios(&menu, vec![Page::Game, Page::Console, Page::System])
        );
        menu.turn_to(Page::Console);
        assert_eq!(
            labels(&menu, &cx),
            ["3D resolution", "View", "Sort by", "Show", "Game case"]
        );
        menu.turn_to(Page::System);
        let system = labels(&menu, &cx);
        assert_eq!(
            system[..6],
            [
                "View",
                "Sort by",
                "Show",
                "Game case",
                "Show systems with no games",
                "Sounds"
            ]
        );
        assert_eq!(system.last().map(String::as_str), Some("Power off"));

        // The GBA page, a second emulator for GBA, no game: the Emulator row, no 3D resolution.
        let mut w = world("menu-gba", true);
        let cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Gba,
            game: None,
        };
        let menu = Menu::open(false);
        assert_eq!(menu.page, Page::Console);
        assert_eq!(
            labels(&menu, &cx),
            ["Emulator", "View", "Sort by", "Show", "Game case"]
        );
        assert_eq!(
            menu.pages(&cx),
            with_radios(&menu, vec![Page::Console, Page::System])
        );
    }

    #[test]
    fn inheriting() {
        let mut w = world("menu-inherit", false);
        let game = w.game.clone();
        let mut cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: Some(&game),
        };
        let mut menu = Menu::open(true);
        let res = 2; // 3D resolution, on the Game page
        let shown = |m: &Menu, cx: &Context| m.shown(cx).rows[res].value.clone();
        let inherited = |s: &str| Value::Inherited(s.into());
        let right = Input {
            turn: 1,
            ..Input::default()
        };
        let reset = Input {
            reset: true,
            ..Input::default()
        };
        // Set nowhere: not passed, so DSperate's own setting holds; no Y to offer.
        assert_eq!(shown(&menu, &cx), inherited("Use DSperate's own"));
        menu.update(&moved(res as i32), &mut cx);
        assert!(!menu.shown(&cx).hint.contains("Y "));

        // Set for the console, on the console's page: "Use ...", then each value in turn.
        menu.turn_to(Page::Console);
        assert!(menu.update(&right, &mut cx).changed);
        assert!(menu.update(&right, &mut cx).changed);
        menu.turn_to(Page::Game);
        menu.update(&moved(res as i32), &mut cx);
        assert_eq!(shown(&menu, &cx), inherited("Use DS (2x)"));

        // Set for the game: its own, Y offered to go back to the console's, and Y does it.
        menu.update(&right, &mut cx);
        assert_eq!(shown(&menu, &cx), Value::Text("1x".into()));
        assert!(menu.shown(&cx).hint.contains("Y  use DS"));
        assert!(menu.update(&reset, &mut cx).changed);
        assert_eq!(shown(&menu, &cx), inherited("Use DS (2x)"));
        assert!(!menu.update(&reset, &mut cx).changed);

        // Round the values and back to "Use DS": the same as Y.
        for _ in 0..4 {
            menu.update(&right, &mut cx);
        }
        assert_eq!(shown(&menu, &cx), inherited("Use DS (2x)"));
        assert!(cx
            .settings
            .get(&Scope::Game(game.key()), "dsperate.hires3d")
            .is_none());

        // The shelf's settings: the system's, which a console's page inherits.
        menu.turn_to(Page::System);
        menu.update(&right, &mut cx);
        menu.turn_to(Page::Console);
        let view = 1;
        assert_eq!(
            menu.shown(&cx).rows[view].value,
            inherited("Use System (Cover grid)")
        );
        assert_eq!(cx.settings.view(Console::Ds), crate::settings::View::Grid);
    }

    /// A command run for a row (switching the speaker tuning): working, then its answer on
    /// the row, and a restart offered for it.
    #[test]
    fn jobs() {
        let mut w = world("menu-jobs", false);
        let mut cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: None,
        };
        let mut menu = Menu::open(false);
        menu.turn_to(Page::System);
        menu.tuning = Some(nibkit::audio::Tuning::Nibos);
        menu.run(Item::SpeakerTuning, || {
            Ok("Speaker tuning: official, from the next restart.".into())
        });
        assert!(menu
            .note
            .as_ref()
            .is_some_and(|(_, n)| n.starts_with("Working")));
        // Another waits for the first.
        menu.run(Item::ImportTuning, || Ok("never".into()));
        for _ in 0..100 {
            if menu.job.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            menu.update(&Input::default(), &mut cx);
        }
        assert!(menu.restart);
        let items = menu.items(&cx);
        assert!(items.contains(&Item::RestartNow));
        let row = items
            .iter()
            .position(|i| *i == Item::SpeakerTuning)
            .unwrap();
        assert_eq!(
            menu.note,
            Some((
                row,
                "Speaker tuning: official, from the next restart.".into()
            ))
        );
    }

    #[test]
    fn actions_ask_first() {
        let mut w = world("menu-actions", false);
        let mut cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: None,
        };
        let mut menu = Menu::open(false);
        menu.turn_to(Page::System);
        let last = menu.items(&cx).len() - 1;
        let press = Input {
            pressed: true,
            turn: 1,
            ..Input::default()
        };
        menu.update(&moved(-1), &mut cx);
        assert_eq!(menu.row, last);
        assert!(!menu.update(&press, &mut cx).power_off);
        let shown = menu.shown(&cx);
        assert!(matches!(
            &shown.rows[shown.row].value,
            Value::Prompt { warning: true, .. }
        ));
        // Moving away disarms it.
        menu.update(&moved(-1), &mut cx);
        menu.update(&moved(1), &mut cx);
        assert!(!menu.update(&press, &mut cx).power_off);
        assert!(menu.update(&press, &mut cx).power_off);
    }

    #[test]
    fn touches() {
        assert_eq!(tab_hit(TABS.0 + 10.0, TABS.1 + 10.0, 3), Some(0));
        assert_eq!(tab_hit(TABS.0 + 2.5 * TABS.2, TABS.1, 3), Some(2));
        assert_eq!(tab_hit(TABS.0 + 3.5 * TABS.2, TABS.1, 3), None);
        assert_eq!(row_hit(row_y(3) + 10.0), Some(3));
        assert_eq!(row_hit(TOP - ROW_STEP), None);
    }

    /// System info: A on its row opens it, its rows are those the device has a source for,
    /// and B goes back to the System page on its row.
    #[test]
    fn system_info() {
        let mut w = world("menu-sysinfo", false);
        let mut cx = Context {
            stats: &mut w.stats,
            settings: &mut w.settings,
            emulators: &w.emulators,
            console: Console::Ds,
            game: None,
        };
        let mut menu = Menu::open(false);
        menu.turn_to(Page::System);
        let at = menu
            .items(&cx)
            .iter()
            .position(|i| *i == Item::SystemInfo)
            .unwrap();
        menu.update(&moved(at as i32), &mut cx);
        let press = Input {
            pressed: true,
            ..Input::default()
        };
        menu.update(&press, &mut cx);
        assert!(menu.reading_sys);
        assert!(menu.shown(&cx).hint.contains("back to System"));

        // The facts as alpha2 gives them, less a kernel release and a battery.
        menu.sys = sysinfo::Info {
            read: true,
            board: Some("Anbernic RG DS+".into()),
            release: Some(sysinfo::release(
                "NIBOS_VERSION=0.1.0-alpha2\nNIBOS_BUILD=b32321d\nNIBOS_EDITION=dslot\n\
                 NIBOS_CHANNEL=stable\n",
            )),
            kernel: None,
            uptime: Some(918),
            root: Some("nibos-root-b".into()),
            memory: Some(sysinfo::Memory {
                total: 1_008_332,
                available: 431_228,
                shared: 216_204,
                cache: 254_332,
                swap_total: 1_008_328,
                swap_used: 0,
                cma: Some((2176, 32768)),
            }),
            swap: Some("zram0".into()),
            rss: Some(127_564),
            load: Some([0.59, 0.70, 0.48]),
            cores: vec![1_416_000; 4],
            governor: Some("schedutil".into()),
            zones: vec![sysinfo::Zone {
                name: "cpu".into(),
                millideg: 43_125,
            }],
            gpu: Some((200_000_000, "simple_ondemand".into())),
            volumes: vec![sysinfo::Volume {
                name: "System",
                used: 1_400_942_592,
                total: 1_767_247_872,
            }],
            panels: vec![("DSI-1".into(), "1024x768".into())],
        };
        assert_eq!(
            labels(&menu, &cx),
            [
                "Board",
                "nibOS",
                "Up for",
                "Slot",
                "DSlot",
                "Memory",
                "Available",
                "DSlot's own",
                "Shared (GPU)",
                "File cache",
                "Swap",
                "CMA",
                "Load",
                "CPU",
                "Temperature",
                "GPU",
                "System",
                "Display",
            ]
        );
        let shown = menu.shown(&cx);
        assert_eq!(shown.title, "nibOS 0.1.0-alpha2 on Anbernic RG DS+");
        let row = |label: &str| shown.rows.iter().find(|r| r.label == label).unwrap();
        let prompt = |text: &str| Value::Prompt {
            text: text.into(),
            warning: false,
        };
        assert_eq!(
            row("nibOS").value,
            prompt("0.1.0-alpha2 \u{b7} dslot \u{b7} stable")
        );
        assert_eq!(row("nibOS").source.as_deref(), Some("build b32321d"));
        assert_eq!(row("Up for").value, prompt("15 min"));
        assert_eq!(row("Slot").value, prompt("B"));
        assert_eq!(row("Memory").value, Value::Slider(57));
        assert_eq!(
            row("Memory").source.as_deref(),
            Some("563 of 984 MB in use")
        );
        assert_eq!(row("DSlot's own").value, prompt("124 MB resident"));
        assert_eq!(row("Swap").value, Value::Slider(0));
        assert_eq!(
            row("Swap").source.as_deref(),
            Some("0 of 984 MB in use on zram0")
        );
        assert_eq!(row("CMA").value, prompt("2 of 32 MB in use"));
        assert_eq!(row("Load").value, prompt("0.59 \u{b7} 0.70 \u{b7} 0.48"));
        assert_eq!(row("CPU").value, prompt("4 \u{d7} 1416 MHz"));
        assert_eq!(row("CPU").source.as_deref(), Some("schedutil"));
        assert_eq!(row("Temperature").value, prompt("CPU 43 \u{b0}C"));
        assert_eq!(row("GPU").value, prompt("200 MHz"));
        assert_eq!(row("System").value, Value::Slider(79));
        assert_eq!(row("System").source.as_deref(), Some("1.3 of 1.6 GB used"));
        assert_eq!(row("Display").value, prompt("DSI-1 1024\u{d7}768"));

        // B: back to the System page, on the System info row.
        let close = Input {
            close: true,
            ..Input::default()
        };
        menu.update(&close, &mut cx);
        assert!(!menu.reading_sys);
        assert_eq!(menu.items(&cx)[menu.row], Item::SystemInfo);
    }

    /// The Update page's row, in each phase: what it says, and when it warns.
    #[test]
    fn update_row_says() {
        let mut menu = Menu::open(false);
        let mut s = Status {
            version: "1.0".into(),
            available: "1.1".into(),
            size: 300_000_000,
            done: 1,
            total: 4,
            detail: "Charge to 30% or plug the charger in.".into(),
            ..Status::default()
        };
        let mut says = |menu: &mut Menu, phase: &str, picked: bool| {
            s.phase = phase.into();
            menu.update = Some((s.clone(), Instant::now()));
            menu.row = if picked { 2 } else { 0 };
            menu.update_value(2)
        };
        let prompt = |text: &str, warning: bool| Value::Prompt {
            text: text.into(),
            warning,
        };
        assert_eq!(says(&mut menu, "", true), prompt("Press A to check", false));
        assert_eq!(
            says(&mut menu, "available", true),
            prompt("Download (300 MB)", false)
        );
        assert_eq!(says(&mut menu, "downloading", false), Value::Slider(25));
        assert_eq!(
            says(&mut menu, "paused", false),
            prompt("Resume (25% done)", false)
        );
        assert_eq!(says(&mut menu, "error", false), prompt("Try again", true));
        // Ready: A once asks, the second time restarts.
        menu.armed = Some(2);
        assert_eq!(
            says(&mut menu, "ready", true),
            prompt("Press A again to restart", true)
        );
    }
}
