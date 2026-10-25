//! The menu, on the touch screen while the top screen keeps showing the selected game. It has
//! a page for each level of settings (settings/mod.rs):
//!
//! - **Game**: the selected game's favourite and rating, the emulator it starts in and that
//!   emulator's options for it, and actions on its files (its resume state, its saves).
//! - **The console** (DS, GBA, ...): the emulator its games start in, that emulator's options,
//!   and how the page is shown.
//! - **System**: how every page is shown, the clicks, button repeat, the device's volume,
//!   brightness and Wi-Fi where DSlot can change them, start at power-on, power off.
//! - **Wi-Fi** and **Bluetooth** (nibOS): the networks in range, joining one (its passphrase
//!   typed on nibkit's keyboard), leaving and forgetting; Bluetooth on and off, the paired
//!   devices to connect and disconnect, forgetting one, and pairing a new one. The work is done
//!   on a thread of its own (`radios.rs`).
//! - **Update** (nibOS): the version, the update channel, checking for an update, downloading
//!   it in the background (paused and resumed), restarting into it, what's new, and going back
//!   to the version before; the work is nibOS's updater's (`update.rs`).
//!
//! A row that can inherit shows where its value comes from ("from GBA", "from System") or that
//! it is set here; Y resets it to what it inherits. L / R or the tabs change page; up and down
//! pick a row, left and right or A (or a tap) change it, B closes the menu.

use super::radios::{Ask, Do, Radios, Reply, Snapshot};
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
    /// Looks in every console's folders again, for games the watch on them missed.
    Rescan,
    /// An emulator's action on the game's files; asks before it acts.
    Action(Action),
    /// Deletes the game's saves folder; asks before it acts.
    DeleteSaves,
    PowerOff,
    /// Wi-Fi: the network joined, or what the Wi-Fi is doing; shown, not changed.
    Joined,
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
            Item::Rescan => "Rescan games",
            Item::Action(a) => &a.label,
            Item::DeleteSaves => "Delete saves",
            Item::PowerOff => "Power off",
            Item::SpeakerTuning => "Speaker tuning",
            Item::ImportTuning => "Import official tuning",
            Item::RestartNow => "Restart now",
            Item::Joined => "Joined",
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
        )
    }
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
            (Page::System, _) => vec![Scope::System],
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
}

/// What the keyboard is typing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Typing {
    /// The passphrase for this network.
    Passphrase(String),
    /// A code a device being paired asked for.
    Code,
}

impl Menu {
    /// Opens on the Game page when there is a game selected, else on the console's.
    pub fn open(game: bool) -> Menu {
        let controls = platform::CONTROLS
            .iter()
            .map(|&c| (c, platform::control(c)))
            .collect();
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
        game.into_iter()
            .chain([Page::Console, Page::System])
            .chain(radios.into_iter().flatten())
            .chain(updates)
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
                items.push(Item::Rescan);
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
            Page::Wifi => {
                if self
                    .controls
                    .iter()
                    .any(|(c, v)| *c == Control::Wifi && v.is_some())
                {
                    items.push(Item::Control(Control::Wifi));
                }
                if let Some(wifi) = &self.radio.wifi {
                    items.extend([Item::Joined, Item::Scan]);
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
        if matches!(self.page, Page::Wifi | Page::Bluetooth) && !matches!(item, Item::Control(_)) {
            let item = item.clone();
            self.radio_row(&item, pressed && was == self.row, turn, input.reset);
        } else if self.page == Page::Update {
            let item = item.clone();
            self.update_row(&item, pressed && was == self.row, turn);
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
        let Some(radios) = &self.radios else { return };
        match (typed, typing) {
            (Typed::Done(pass), Typing::Passphrase(name)) => radios.ask(Do::Join(name, Some(pass))),
            (Typed::Done(code), Typing::Code) => radios.reply(Reply::Code(code)),
            (Typed::Cancel, Typing::Code) => radios.reply(Reply::No),
            (Typed::Cancel, Typing::Passphrase(_)) => {}
        }
    }

    fn turn_to(&mut self, page: Page) {
        if self.page != page {
            (self.page, self.row, self.scroll, self.armed, self.note) = (page, 0, 0, None, None);
            (self.forget, self.reading_notes) = (None, false);
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
        if self.page == Page::System {
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
            | Item::Joined
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
            (Page::Wifi | Page::Bluetooth, _) => radio_title(),
            (Page::Game, Some(game)) => game.name().to_owned(),
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
            (Page::Wifi, Some(Item::Network(_))) => {
                "A  join or leave   \u{b7}   Y  forget   \u{b7}   L / R  page   \u{b7}   B  back"
                    .into()
            }
            (Page::Bluetooth, Some(Item::Gadget { .. })) => {
                "A  connect, disconnect or pair   \u{b7}   Y  forget   \u{b7}   B  back".into()
            }
            (Page::Wifi | Page::Bluetooth, _) => {
                "A  choose   \u{b7}   L / R  page   \u{b7}   B  back".into()
            }
            (Page::Update, _) if self.reading_notes => {
                "A  back to Update   \u{b7}   L / R  page   \u{b7}   B  close".into()
            }
            (Page::Update, _) => "A  choose   \u{b7}   L / R  page   \u{b7}   B  back".into(),
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
            _ => None,
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
    /// or BlueZ to talk to (a Linux build machine may).
    fn with_radios(menu: &Menu, mut pages: Vec<Page>) -> Vec<Page> {
        if menu.radios.is_some() {
            pages.extend([Page::Wifi, Page::Bluetooth]);
        }
        pages
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
