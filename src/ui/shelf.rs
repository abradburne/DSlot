//! The shelf, a frame at a time: read the buttons and the touch panel, move the carts, ask
//! for artwork, draw both screens. It runs until a cart is pushed into the slot (the game
//! starts once the click has played and the screens have faded), DSlot is left, or the
//! device is powered off.

use super::bouncer::{self, Bouncer};
use super::draw::{Blade, Page, Ui, Yours, DIM, THUMBS, WHITE};
use super::input::{self, Input, Key, Reader};
use super::layout::*;
use super::menu::{self, Menu};
use super::pads::Pads;
use super::power::{Choice, PowerMenu};
use super::sound::{self, Sound};
use super::{Back, Outcome, Screens, Shelf};
use crate::gfx::save_png;
use crate::library::{self, Filter};
use crate::loader::Job;
use crate::platform;
use crate::rescan::Found;
use crate::rom::{Console, Game};
use crate::settings::{SaverLook, Settings, Show, Sort, View as Layout};
use crate::stats::Stats;
use nibkit::touch::{Panel, Pointer};
use sdl2::event::Event;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Pushing a cart in: the slide, then the fade to black once the latch has clicked.
const INSERT_SECS: f32 = 0.42;
const FADE_SECS: f32 = 0.2;
/// Coming back from a game: the cart thrown clear of the slot.
const EJECT_SECS: f32 = 0.28;
/// How deep the cart sits in the slot, and how much further a push takes it before it latches.
const SEAT: f32 = 560.0;
const PRESS: f32 = 16.0;
const SPRING_SECS: f32 = 0.06;
/// How quickly the carts and the grid catch up with the selection, per second.
const FOLLOW: f32 = 14.0;
/// A flick of the grid or the list carries on after the finger lifts, slowing as iOS's lists
/// do (its normal deceleration, 0.998 a millisecond), from at least this fast and at most this
/// fast, in screen units a second; it stops once slower than the last.
const FLING_DECAY: f32 = 2.0;
const FLING_MIN: f32 = 300.0;
const FLING_MAX: f32 = 8000.0;
const FLING_STOP: f32 = 10.0;
/// How far back a finger's movement counts toward how fast it lifted: one that stopped before
/// it lifted throws nothing.
const FLING_WINDOW: Duration = Duration::from_millis(80);
/// Past either end, the list slows this much harder, and springs back once stopped.
const FLING_PAST_END: f32 = 30.0;

/// How quickly the tab strip scrolls to a page's tab, per second: there in about 120 ms.
const STRIP_FOLLOW: f32 = 28.0;

/// Started early at power-on, the touch panel's driver may still be loading: look again this
/// many frames apart (about half a second), for this many frames (about half a minute).
const PANEL_RETRY: u32 = 30;
const PANEL_RETRY_FOR: u32 = 1800;

/// Shows the shelf until a game is chosen or DSlot is left. `back` is the game just played, if
/// any: it is selected, and its cart pops out of the slot (or, put away for the power menu,
/// the menu opens).
pub fn run(
    screens: Screens,
    shelf: &Shelf,
    games: &mut Vec<Game>,
    back: Option<Back>,
    stats: &mut Stats,
    settings: &mut Settings,
) -> Result<Outcome, String> {
    // The artwork workers live exactly as long as the shelf is up.
    std::thread::scope(|scope| {
        let Screens {
            canvases, top, bot, ..
        } = screens;
        let Shelf {
            fonts, shell, opts, ..
        } = *shelf;
        let ui = Ui::new(
            scope,
            canvases,
            (top, bot),
            (fonts, shell, &opts.cache),
            settings.look(settings.page),
            shelf.ra,
        )?;
        let screens = Frame {
            sdl: screens.sdl,
            dual: screens.dual,
            headless: screens.headless,
        };
        show(ui, screens, shelf, games, back, stats, settings)
    })
}

/// What `run` keeps of the screens once the canvases are the `Ui`'s.
struct Frame {
    sdl: sdl2::Sdl,
    dual: bool,
    headless: bool,
}

/// A touch that has come down and not yet lifted.
struct Drag {
    start: (f32, f32),
    at: (f32, f32),
    /// Where the carts and the grid were when it came down.
    carousel: f32,
    scroll: f32,
    /// It came down on the centre cart (which it can pull into the slot).
    on_cart: bool,
    /// It came down on a grid or list still moving from a flick: it stops it, and picks nothing.
    caught: bool,
    /// Where it has been down the screen, and when, lately: how fast it lifts.
    track: Vec<(Instant, f32)>,
}

impl Drag {
    fn moved(&self) -> (f32, f32) {
        (self.at.0 - self.start.0, self.at.1 - self.start.1)
    }

    fn is_tap(&self) -> bool {
        let (dx, dy) = self.moved();
        dx.abs() < TAP_SLOP && dy.abs() < TAP_SLOP
    }

    /// Notes where the finger is now, forgetting what is too old to count.
    fn note(&mut self, at: Instant) {
        self.track.push((at, self.at.1));
        let from = at.checked_sub(FLING_WINDOW * 2).unwrap_or(at);
        self.track.retain(|&(t, _)| t >= from);
    }
}

/// How fast a finger was moving down the screen as it lifted at `at`, in screen units a second:
/// over its last moments, so that a finger that stopped before lifting throws nothing.
fn release_speed(track: &[(Instant, f32)], at: Instant) -> f32 {
    let from = at.checked_sub(FLING_WINDOW).unwrap_or(at);
    let recent: Vec<&(Instant, f32)> = track.iter().filter(|(t, _)| *t >= from).collect();
    let (Some(first), Some(last)) = (recent.first(), recent.last()) else {
        return 0.0;
    };
    let secs = at.duration_since(first.0).as_secs_f32();
    if secs < 0.01 {
        return 0.0;
    }
    (last.1 - first.1) / secs
}

/// One frame of a flicked grid or list coasting: `scroll` rows down at `speed` rows a second,
/// `dt` seconds on, between 0 and `most`. Returns where it is, and its speed if it still moves.
fn coast(scroll: f32, speed: f32, dt: f32, most: f32, row_h: f32) -> (f32, Option<f32>) {
    let scroll = scroll + speed * dt;
    let past = scroll < 0.0 || scroll > most;
    let decay = FLING_DECAY + if past { FLING_PAST_END } else { 0.0 };
    let speed = speed * (-decay * dt).exp();
    let scroll = scroll.clamp(-0.5, most + 0.5);
    (scroll, (speed.abs() * row_h > FLING_STOP).then_some(speed))
}

/// A finger dragging the grid or list past either end moves it less and less, as on iOS:
/// `over` rows past the end, of a screen of `on_screen` rows, shows as this many.
fn rubber(over: f32, on_screen: f32) -> f32 {
    (1.0 - 1.0 / (over * 0.55 / on_screen + 1.0)) * on_screen
}

/// Where the shelf is: its page, the selection, and everything that moves.
struct State {
    /// The consoles with pages, in tab order: those DSlot can draw carts for.
    pages: Vec<Console>,
    /// The games on show, in order, as indices into `games`. `sel` and `carousel` are places
    /// in this list.
    order: Vec<usize>,
    sel: usize,
    /// Where the row of carts is: `sel` when it has caught up, between two while it moves.
    carousel: f32,
    /// How many rows down the cover grid or the list is scrolled.
    scroll: f32,
    /// The grid or list moving on its own after a flick, in rows a second.
    fling: Option<f32>,
    /// Seconds since the selected cart started into the slot, and whether its click is queued.
    insert: Option<f32>,
    clicked: bool,
    /// Seconds since the cart started out of the slot (from below 0: the click's lead).
    eject: Option<f32>,
    popped: bool,
    drag: Option<Drag>,
    /// The D-pad (or stick) held: it repeats, on the shelf, the grid and the menus.
    repeat: nibkit::input::Repeat<Key>,
    menu: Option<Menu>,
    /// The power menu, over everything else.
    power: Option<PowerMenu>,
    /// Why the game just left came straight back, over the shelf until any button is pressed.
    notice: Option<Notice>,
    /// The game last selected on each page, by index into `games`.
    kept: [Option<usize>; Console::COUNT],
    /// A page to turn to at the top of the next frame.
    turn_page: Option<Console>,
    /// The tab strip: how far it is scrolled and how far it is heading, and the page (and
    /// how many pages there were) it was last centred for.
    strip: (f32, f32),
    strip_on: Option<(Console, usize)>,
    /// The system picker (Select), open.
    picker: Option<Picker>,
    /// The page the shelf had to leave (its games gone, or not there yet at the start): turned
    /// back to when its games are, unless another page has been chosen since.
    away_from: Option<Console>,
}

/// The notice of a game that came straight back: why, and a bug report about it, where one can
/// be sent from here (nibOS: bugreport.rs).
struct Notice {
    title: &'static str,
    text: String,
    report: Report,
}

/// Where the notice's bug report is.
enum Report {
    /// Can't be sent from here: the notice only says what happened.
    NotHere,
    /// Y sends it.
    Offered,
    /// On its way; the outcome comes on the receiver.
    Sending(std::sync::mpsc::Receiver<crate::bugreport::Outcome>),
    Done(crate::bugreport::Outcome),
}

impl Notice {
    fn new(text: String) -> Notice {
        let report = if crate::bugreport::available() {
            Report::Offered
        } else {
            Report::NotHere
        };
        Notice {
            title: "The game stopped",
            text,
            report,
        }
    }

    /// A notice nibOS left for the shelf (notices.rs): its text, nothing to send.
    fn from_system(text: String) -> Notice {
        Notice {
            title: "A notice from nibOS",
            text,
            report: Report::NotHere,
        }
    }

    /// A button or a touch: Y sends the report when it can be; while it is on its way nothing
    /// happens; anything else closes the notice. Returns whether it stays.
    fn press(&mut self, key: Option<Key>) -> bool {
        match (&self.report, key) {
            (Report::Offered, Some(Key::Y)) => {
                self.report = Report::Sending(crate::bugreport::send(&self.text));
                true
            }
            (Report::Sending(_), _) => true,
            _ => false,
        }
    }

    /// Takes in the outcome of a report on its way, if it has come.
    fn poll(&mut self) {
        if let Report::Sending(rx) = &self.report {
            match rx.try_recv() {
                Ok(outcome) => self.report = Report::Done(outcome),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                // The thread is gone without a word: it says so on stderr.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.report = Report::Done(crate::bugreport::Outcome::Failed { file: None })
                }
            }
        }
    }

    fn sending(&self) -> bool {
        matches!(self.report, Report::Sending(_))
    }

    /// What the box shows under the reason: the report's outcome, and the line of hints.
    fn shown(&self) -> super::draw::NoticeShown<'_> {
        use super::draw::NoticeShown;
        match &self.report {
            Report::NotHere => NoticeShown {
                title: self.title,
                text: &self.text,
                outcome: None,
                code: None,
                hint: "any button closes this",
            },
            Report::Offered => NoticeShown {
                title: self.title,
                text: &self.text,
                outcome: None,
                code: None,
                hint: "Y  send a bug report   \u{b7}   any other button closes this",
            },
            Report::Sending(_) => NoticeShown {
                title: self.title,
                text: &self.text,
                outcome: Some("Sending the report...".to_owned()),
                code: None,
                hint: "",
            },
            Report::Done(outcome) => NoticeShown {
                title: self.title,
                text: &self.text,
                outcome: Some(outcome.words()),
                code: outcome.code(),
                hint: "any button closes this",
            },
        }
    }
}

/// The system picker, open: the blade chosen, where the stack is as it glides to it, and a
/// finger on it: where it came down, where the stack was then, and where it is now.
#[derive(Clone, Copy)]
struct Picker {
    at: usize,
    pos: f32,
    drag: Option<(f32, f32, f32)>,
}

impl Picker {
    fn on(at: usize) -> Picker {
        Picker {
            at,
            pos: at as f32,
            drag: None,
        }
    }

    /// `d` blades along, held at the ends.
    fn step(&mut self, d: i32, n: usize) {
        self.at = (self.at as i32 + d).clamp(0, n as i32 - 1) as usize;
    }
}

/// The screensaver, on: the logo drifting (or still, or the screens off), the screens dimmed
/// (put back as they were when it is dropped).
struct Saver {
    bouncer: Bouncer,
    /// The screens the logo was on last frame: they want drawing again, to clear it away.
    shown: [bool; 2],
    look: SaverLook,
    /// When the logo was last drawn, and whether it has been yet.
    drawn: Option<Instant>,
    _dimmed: Option<nibkit::backlight::Dimmed>,
    _off: Option<ScreensOff>,
}

/// Both screens' backlights off, for as long as it is kept: on again when it is dropped.
struct ScreensOff;

impl ScreensOff {
    fn now() -> ScreensOff {
        for screen in [
            nibkit::screens::Screen::Top,
            nibkit::screens::Screen::Bottom,
        ] {
            nibkit::backlight::set_off(screen, true);
        }
        ScreensOff
    }
}

impl Drop for ScreensOff {
    fn drop(&mut self) {
        for screen in [
            nibkit::screens::Screen::Top,
            nibkit::screens::Screen::Bottom,
        ] {
            nibkit::backlight::set_off(screen, false);
        }
    }
}

/// How bright the screens go while the screensaver is on, in percent (no brighter than they
/// were). Each frame of the drifting logo costs the GPU a wake-up and a present, so it is drawn
/// a few times a second (`DSLOT_SAVER_FPS` overrides it, for measuring); the still logo moves
/// once a minute. Input is looked for every `REST_POLL` meanwhile.
const SAVER_DIM: u8 = 10;
const SAVER_FPS: f32 = 15.0;
const SAVER_STILL: Duration = Duration::from_secs(60);

fn saver_frame() -> Duration {
    let fps = std::env::var("DSLOT_SAVER_FPS")
        .ok()
        .and_then(|f| f.parse::<f32>().ok())
        .filter(|f| *f > 0.0)
        .unwrap_or(SAVER_FPS);
    Duration::from_secs_f32(1.0 / fps.min(60.0))
}
/// How long after the last touch the shelf rests (see `resting` in `show`), and how often it
/// looks for input meanwhile.
const REST_AFTER: Duration = Duration::from_secs(3);
const REST_POLL: Duration = Duration::from_millis(50);
/// A gap between frames this long means the system slept (the lid, the power button).
const SLEPT: Duration = Duration::from_secs(2);

/// Somewhere new for the logo: a fraction of its room along each axis.
fn somewhere() -> (f32, f32) {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_millis() + d.as_secs() as u32 * 7);
    ((t % 97) as f32 / 97.0, (t % 89) as f32 / 89.0)
}

impl Saver {
    fn start(look: SaverLook) -> Saver {
        crate::mark("screensaver on");
        Saver {
            bouncer: Bouncer::new(bouncer::AREA, bouncer::LOGO, somewhere()),
            // The shelf is on both: clear it away first.
            shown: [true, true],
            look,
            drawn: None,
            _dimmed: nibkit::backlight::dim(SAVER_DIM),
            _off: (look == SaverLook::Off).then(ScreensOff::now),
        }
    }

    /// Whether the logo is to be drawn now, moved on to where it is now.
    fn due(&mut self) -> bool {
        let since = self.drawn.map(|t| t.elapsed());
        match self.look {
            SaverLook::Off => false,
            SaverLook::Animated => {
                let frame = saver_frame();
                match since {
                    Some(s) if s < frame => false,
                    // Moved by the time since the last frame, in steps short enough to meet each
                    // wall at most once.
                    Some(s) => {
                        let mut left = s.as_secs_f32().min(2.0);
                        while left > 0.0 {
                            self.bouncer.step(left.min(0.1));
                            left -= 0.1;
                        }
                        true
                    }
                    None => true,
                }
            }
            SaverLook::Still => match since {
                Some(s) if s < SAVER_STILL => false,
                Some(_) => {
                    let hue = self.bouncer.hue;
                    self.bouncer = Bouncer::new(bouncer::AREA, bouncer::LOGO, somewhere());
                    self.bouncer.hue = (hue + 0.17).fract();
                    true
                }
                None => true,
            },
        }
    }
}

/// Whether an event is someone at the device: a button, a key, a touch.
fn woken_by(event: &Event) -> bool {
    matches!(
        event,
        Event::KeyDown { .. }
            | Event::ControllerButtonDown { .. }
            | Event::JoyButtonDown { .. }
            | Event::MouseButtonDown { .. }
            | Event::FingerDown { .. }
    )
}

fn filter(settings: &Settings) -> Filter {
    let console = settings.page;
    let faves_only = settings.show(console) == Show::Favourites;
    Filter {
        console,
        sort: settings.sort(console),
        faves_only,
    }
}

/// How soon a held D-pad repeats, and then how often (the Button repeat setting).
/// How `console`'s page is shown: its view, but a page DSlot draws no carts for (Game & Watch,
/// disks and discs, the apps) shows its boxes in the grid in place of them.
fn mode(console: Console, settings: &Settings) -> Layout {
    match settings.view(console) {
        Layout::Shelf if !crate::art::has_shell(console) => Layout::Grid,
        view => view,
    }
}

/// A view that scrolls in rows (all but the carts): games to a row, rows on screen, and a
/// row's height.
fn rows(mode: Layout) -> Option<(usize, usize, f32)> {
    match mode {
        Layout::Shelf => None,
        Layout::Grid => Some((COLS, GRID_ROWS, GRID_STEP.1)),
        Layout::List | Layout::ListCovers => {
            let (h, on_screen) = list_rows(mode == Layout::ListCovers);
            Some((1, on_screen, h))
        }
    }
}

/// The shelf's pages, in tab order: each console with games on a card (with Show systems with
/// no games, every console looked for), then the apps when there are any. With no games on
/// any console, every console looked for: each page says where its games go.
fn pages(
    games: &[Game],
    searched: &[Vec<PathBuf>; Console::COUNT],
    show_empty: bool,
) -> Vec<Console> {
    let has = |c: Console| games.iter().any(|g| g.console == c && !g.gone);
    let looked = |c: Console| !searched[c as usize].is_empty();
    let any = Console::ALL.into_iter().any(|c| looked(c) && has(c));
    Console::PAGES
        .into_iter()
        .filter(|&c| match c {
            Console::Apps => has(c),
            c => looked(c) && (has(c) || show_empty || !any),
        })
        .collect()
}

/// A page's blade in the system picker: its games, the game whose art it shows (the one last
/// played, else the first by name), and when that was.
fn blade(games: &[Game], stats: &Stats, console: Console) -> Blade {
    let mine = || {
        games
            .iter()
            .enumerate()
            .filter(move |(_, g)| g.console == console && !g.gone)
    };
    let last = mine()
        .map(|(i, g)| (stats.last(&g.path), i))
        .filter(|&(t, _)| t > 0)
        .max();
    let game = last.map(|(_, i)| i).or_else(|| {
        mine()
            .min_by_key(|(_, g)| g.name().to_lowercase())
            .map(|(i, _)| i)
    });
    Blade {
        console,
        games: mine().count(),
        last: last.map(|(_, i)| stats.last_played(&games[i].path)),
        art: game.is_some_and(|i| games[i].art("Imgs").is_file()),
        game,
    }
}

/// What a system's card says of the player's own games for it: how many are on the cards, how
/// long they have been played in all, and the one played longest.
/// Where `game`'s resume point is, by the emulator it starts in, if that is one whose resume
/// points DSlot can read (resume.rs) and DSlot gives it its saves folder.
fn resume_of(shelf: &Shelf, settings: &Settings, game: &Game) -> Option<crate::resume::Wanted> {
    let root = crate::platform::saves()?;
    let emulator = crate::launch::emulator_for(shelf.emulators, settings, game)?;
    let command = emulator.command(game.console.folder())?;
    if !command.iter().any(|w| w.contains("{saves}")) {
        return None;
    }
    Some(crate::resume::Wanted {
        saves: crate::launch::saves_dir(&root, game),
        stem: game.stem.clone(),
        by: crate::resume::By::of(&emulator.name, command)?,
    })
}

/// A button as the on-screen keyboard takes it.
fn keyboard_key(key: Key) -> Option<nibkit::menu::keyboard::Key> {
    use nibkit::menu::keyboard::Key as K;
    Some(match key {
        Key::Up => K::Up,
        Key::Down => K::Down,
        Key::Left => K::Left,
        Key::Right => K::Right,
        Key::A => K::A,
        Key::B => K::B,
        Key::X => K::X,
        Key::Y => K::Y,
        Key::L => K::L,
        Key::R => K::R,
        Key::Start => K::Start,
        Key::Select => K::Select,
    })
}

/// An overlay that failed to draw: logged the first time, and the frame goes on without it
/// rather than DSlot stopping.
fn soft(what: &str, drawn: Result<(), String>) {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Err(e) = drawn {
        if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("dslot: {what} not drawn: {e}");
        }
    }
}

fn yours(games: &[Game], stats: &Stats, console: Console) -> Yours {
    let mine: Vec<&Game> = games
        .iter()
        .filter(|g| g.console == console && !g.gone)
        .collect();
    // An app's play time says little: the one used last, for apps.
    let (by, most_is): (fn(&Stats, &Game) -> i64, _) = if console == Console::Apps {
        (|s, g| s.last(&g.path), "Last used")
    } else {
        (|s, g| s.secs(&g.path) as i64, "Most played")
    };
    let most = mine
        .iter()
        .map(|g| (by(stats, g), g))
        .filter(|&(n, _)| n > 0)
        .max_by_key(|&(n, _)| n)
        .map(|(_, g)| g.name().to_owned());
    Yours {
        games: mine.len(),
        secs: mine.iter().map(|g| stats.secs(&g.path)).sum(),
        most,
        most_is,
    }
}

/// The page nearest `page` in tab order, of `pages`: the one after it on a tie.
fn nearest(page: Console, pages: &[Console]) -> Console {
    let at = |c: Console| Console::PAGES.iter().position(|&p| p == c).unwrap_or(0) as i32;
    pages
        .iter()
        .copied()
        .min_by_key(|&p| ((at(p) - at(page)).abs(), at(p) < at(page)))
        .unwrap_or(Console::Ds)
}

fn repeat_times(settings: &Settings) -> (Duration, Duration) {
    let (first, next) = settings.repeat().millis();
    (Duration::from_millis(first), Duration::from_millis(next))
}

impl State {
    fn new(
        games: &[Game],
        stats: &Stats,
        settings: &mut Settings,
        back: Option<Back>,
        searched: &[Vec<PathBuf>; Console::COUNT],
        ra: &crate::achievements::Ra,
    ) -> State {
        let back_from = back.as_ref().map(|b| b.game);
        let put_away = back.as_ref().filter(|b| b.put_away).map(|b| b.game);
        let notice = back.and_then(|b| b.notice).map(Notice::new);
        let pages = pages(games, searched, settings.show_empty());
        // Back from a game: on the page it was on.
        if let Some(g) = back_from.and_then(|i| games.get(i)) {
            settings.page = g.console;
        }
        // A page with no games (its card not in yet, say): the nearest, until it has some.
        let mut away_from = None;
        if !pages.contains(&settings.page) {
            away_from = Some(settings.page);
            settings.page = nearest(settings.page, &pages);
        }
        let order = library::order(games, stats, filter(settings));
        let sel = order
            .iter()
            .position(|&g| back_from == Some(g))
            .unwrap_or(0);
        let game = order.get(sel).is_some();
        // The cart pops out of the slot; unless the power menu is what the game was put away for.
        let eject = (back_from.is_some() && put_away.is_none() && !order.is_empty())
            .then_some(-sound::EJECT_WARMUP);
        State {
            pages,
            scroll: (sel / COLS).saturating_sub(1) as f32,
            fling: None,
            carousel: sel as f32,
            order,
            sel,
            insert: None,
            clicked: false,
            eject,
            popped: false,
            drag: None,
            repeat: {
                let (first, next) = repeat_times(settings);
                nibkit::input::Repeat::new(first, next)
            },
            // After an update (or one that didn't start), its notice, on the Update page.
            menu: (put_away.is_none() && crate::update::notice())
                .then(|| Menu::open_update(game, ra)),
            power: put_away.map(|g| PowerMenu::open(Some(g))),
            notice,
            kept: [None; Console::COUNT],
            turn_page: None,
            strip: (0.0, 0.0),
            strip_on: None,
            picker: None,
            away_from,
        }
    }

    /// Works out the pages again (games came or went, or Show systems with no games changed):
    /// off a page that has gone, to the nearest; back to the one left that way, once it is here.
    fn repage(
        &mut self,
        games: &[Game],
        searched: &[Vec<PathBuf>; Console::COUNT],
        settings: &Settings,
    ) {
        self.pages = pages(games, searched, settings.show_empty());
        let here = settings.page;
        if let Some(back) = self.away_from.filter(|p| self.pages.contains(p)) {
            self.away_from = None;
            if back != here {
                self.turn_page = Some(back);
            }
        } else if !self.pages.contains(&here) {
            self.away_from.get_or_insert(here);
            self.turn_page = Some(nearest(here, &self.pages));
        }
        self.picker = None;
    }

    /// The tab strip for this frame, beside a heading `heading` wide on `page`: it scrolls to
    /// the page's tab when the page (or the number of pages) changes, briefly, and at once the
    /// first time.
    fn strip(&mut self, heading: f32, page: Console, dt: f32) -> Strip {
        let mut strip = Strip::new(self.pages.len(), heading, self.strip.0);
        let on = Some((page, self.pages.len()));
        if self.strip_on != on {
            let at = self.pages.iter().position(|&p| p == page).unwrap_or(0);
            self.strip.1 = strip.centred(at);
            if self.strip_on.is_none() {
                self.strip.0 = self.strip.1;
            }
            self.strip_on = on;
        }
        let to = self.strip.1.clamp(0.0, strip.most());
        self.strip.0 += (to - self.strip.0) * (1.0 - (-dt * STRIP_FOLLOW).exp());
        if (to - self.strip.0).abs() < 0.5 {
            self.strip.0 = to;
        }
        strip.scroll = self.strip.0.clamp(0.0, strip.most());
        strip
    }

    /// A cart is going into or coming out of the slot: nothing else moves meanwhile.
    fn busy(&self) -> bool {
        self.insert.is_some() || self.eject.is_some()
    }

    fn last(&self) -> usize {
        self.order.len().saturating_sub(1)
    }

    /// `d` places from `sel`, held at the ends.
    fn step(&self, d: i32) -> usize {
        (self.sel as i32 + d).clamp(0, self.last() as i32) as usize
    }

    /// Up and down: a row in the grid or the list, a letter of the alphabet on the shelf.
    fn vertical(&self, games: &[Game], d: i32, mode: Layout) -> usize {
        match rows(mode) {
            Some((per_row, ..)) => self.step(d * per_row as i32),
            None => self.letter(games, d),
        }
    }

    /// Left and right: a letter of the alphabet in the list, the next game everywhere else.
    fn horizontal(&self, games: &[Game], d: i32, mode: Layout) -> usize {
        match mode {
            Layout::List | Layout::ListCovers => self.letter(games, d),
            Layout::Shelf | Layout::Grid => self.step(d),
        }
    }

    fn letter(&self, games: &[Game], d: i32) -> usize {
        library::letter_jump(games, &self.order, self.sel, d)
    }

    /// The page `d` tabs along from this one, round from the last to the first.
    fn beside(&self, page: Console, d: i32) -> Console {
        let at = self.pages.iter().position(|&s| s == page).unwrap_or(0) as i32;
        self.pages[(at + d).rem_euclid(self.pages.len() as i32) as usize]
    }

    /// Turns to `to`'s page, on the game last selected there.
    fn turn_to(&mut self, games: &[Game], to: Console, stats: &Stats, settings: &mut Settings) {
        self.kept[settings.page as usize] = self.order.get(self.sel).copied();
        settings.page = to;
        settings.save();
        crate::mark(&format!("page {}", to.label()));
        self.order = library::order(games, stats, filter(settings));
        self.sel = self.kept[to as usize]
            .and_then(|g| self.order.iter().position(|&o| o == g))
            .unwrap_or(0);
        self.carousel = self.sel as f32;
        self.scroll = rows(mode(to, settings)).map_or(0.0, |(per_row, ..)| {
            (self.sel / per_row).saturating_sub(1) as f32
        });
        self.repeat.stop();
        self.drag = None;
        self.fling = None;
    }

    /// Lists the page again (a favourite, a rating, the sort or the filter changed), staying
    /// on the same game where it is still listed.
    fn relist(&mut self, games: &[Game], stats: &Stats, settings: &Settings) {
        let was = self.order.get(self.sel).copied();
        self.order = library::order(games, stats, filter(settings));
        let sel = was.and_then(|g| self.order.iter().position(|&o| o == g));
        self.sel = sel.unwrap_or(self.sel.min(self.last()));
        self.carousel = self.sel as f32;
        self.repeat.stop();
        self.drag = None;
        self.fling = None;
    }

    /// Lists the page again after games came or went on a card: the same game stays selected
    /// (or, if it went, the one now in its place), and nothing on screen moves on its account.
    fn rescanned(&mut self, games: &[Game], stats: &Stats, settings: &Settings) {
        let was = self.order.get(self.sel).copied();
        self.order = library::order(games, stats, filter(settings));
        let sel = was
            .and_then(|g| self.order.iter().position(|&o| o == g))
            .unwrap_or(self.sel.min(self.last()));
        // The carts, the grid and the list keep where they are relative to the selection.
        self.carousel += sel as f32 - self.sel as f32;
        if let Some((per_row, ..)) = rows(mode(settings.page, settings)) {
            self.scroll += (sel / per_row) as f32 - (self.sel / per_row) as f32;
        }
        self.sel = sel;
    }

    /// A direction pressed: it repeats while held. Returns its way, -1 or 1.
    fn hold(&mut self, key: Key) -> i32 {
        self.repeat.press(key, Instant::now());
        self.fling = None;
        if matches!(key, Key::Up | Key::Left) {
            -1
        } else {
            1
        }
    }

    /// A button on the shelf (not in the menu). Returns whether it asks for a favourite.
    fn press(&mut self, games: &[Game], key: Key, mode: Layout) -> bool {
        match key {
            Key::Left | Key::Right => {
                let dir = self.hold(key);
                self.sel = self.horizontal(games, dir, mode);
            }
            Key::Up | Key::Down => {
                let dir = self.hold(key);
                self.sel = self.vertical(games, dir, mode);
            }
            Key::A | Key::B | Key::Start => (self.insert, self.fling) = (Some(0.0), None),
            Key::Y => return true,
            Key::X | Key::L | Key::R | Key::Select => {}
        }
        false
    }

    /// The direction held, once more, when a repeat is due: none while a cart moves (and none
    /// stored up for after).
    fn repeat_due(&mut self) -> Option<(Key, i32)> {
        if self.busy() {
            return None;
        }
        let key = self.repeat.due(Instant::now())?;
        Some((
            key,
            if matches!(key, Key::Up | Key::Left) {
                -1
            } else {
                1
            },
        ))
    }

    /// Touches on the carts, the grid or the list: drag to browse or scroll, tap to pick, pull
    /// the centre cart down (or tap the picked cover or name) to play.
    fn touch(
        &mut self,
        pointers: &[Pointer],
        mode: Layout,
        (cart_w, cart_h, pitch): (f32, f32, f32),
    ) {
        for ptr in pointers {
            match *ptr {
                Pointer::Down(x, y) => {
                    let on_cart = mode == Layout::Shelf
                        && (x - SW / 2.0).abs() < cart_w / 2.0
                        && (y - CART_Y).abs() < cart_h / 2.0;
                    let mut drag = Drag {
                        start: (x, y),
                        at: (x, y),
                        carousel: self.carousel,
                        scroll: self.scroll,
                        on_cart,
                        caught: self.fling.take().is_some(),
                        track: Vec::new(),
                    };
                    drag.note(Instant::now());
                    self.drag = Some(drag);
                }
                Pointer::Move(x, y) => {
                    if let Some(d) = self.drag.as_mut() {
                        d.at = (x, y);
                        d.note(Instant::now());
                    }
                }
                Pointer::Up => {
                    if let Some(mut d) = self.drag.take() {
                        d.note(Instant::now());
                        self.lift(&d, mode, pitch);
                    }
                }
            }
        }
    }

    /// A touch lifted.
    fn lift(&mut self, d: &Drag, mode: Layout, pitch: f32) {
        let (dx, dy) = d.moved();
        let (n, last) = (self.order.len(), self.last());
        if let Some((_, _, row_h)) = rows(mode) {
            if d.is_tap() && d.caught {
                // A tap on a list still moving only stops it.
            } else if d.is_tap() {
                // A tap picks a cover or a name; a tap on the one already picked plays it.
                let hit = match mode {
                    Layout::Grid => grid_hit(d.start, self.scroll),
                    _ => list_hit(d.start, self.scroll, mode == Layout::ListCovers),
                };
                match hit.filter(|&i| i < n) {
                    Some(i) if i == self.sel => self.insert = Some(0.0),
                    Some(i) => self.sel = i,
                    None => {}
                }
            } else {
                // Flicked: it carries on, slowing. Dragged and let go: it stays.
                let speed = release_speed(&d.track, Instant::now());
                if speed.abs() >= FLING_MIN {
                    self.fling = Some(-speed.clamp(-FLING_MAX, FLING_MAX) / row_h);
                }
                self.keep_in_view(mode);
            }
        } else if d.on_cart && dy > PULL_TO_INSERT && dy > dx.abs() {
            self.insert = Some(0.0);
        } else if d.is_tap() {
            // A tap on a neighbouring cart brings it to the centre.
            let side = ((d.start.0 - SW / 2.0) / pitch).round() as i32;
            self.sel = self.step(side);
        } else {
            self.sel = self.carousel.round().clamp(0.0, last as f32) as usize;
        }
    }

    /// Keeps the selection on the rows of the grid or list in view (in the list, a row in
    /// from either edge where it can be), in its column: as a finger or a flick scrolls them,
    /// the top screen shows a game that is on screen.
    fn keep_in_view(&mut self, mode: Layout) {
        let Some((per_row, on_screen, _)) = rows(mode) else {
            return;
        };
        let rows_in = self.order.len().div_ceil(per_row).max(1) as f32;
        let margin = if per_row == 1 { 1.0 } else { 0.0 };
        let first = self.scroll.ceil().max(0.0);
        let last = (self.scroll + on_screen as f32 - 1.0)
            .floor()
            .min(rows_in - 1.0);
        let (lo, hi) = if last - first >= 2.0 * margin {
            (first + margin, last - margin)
        } else {
            (first, last.max(first))
        };
        let row = ((self.sel / per_row) as f32).clamp(lo, hi) as usize;
        self.sel = (row * per_row + self.sel % per_row).min(self.last());
    }

    /// Moves the carts, the grid or the list toward the selection, or with a finger. Returns
    /// how far the centre cart is pulled toward the slot.
    fn follow(&mut self, dt: f32, mode: Layout, pitch: f32) -> f32 {
        let (n, last) = (self.order.len(), self.last() as f32);
        let ease = 1.0 - (-dt * FOLLOW).exp();
        let mut pull = 0.0;
        match self.drag.as_ref().filter(|_| mode == Layout::Shelf) {
            Some(d) if d.on_cart && d.moved().1 > d.moved().0.abs() => pull = d.moved().1,
            Some(d) if d.moved().0.abs() > TAP_SLOP => {
                self.carousel = (d.carousel - d.moved().0 / pitch).clamp(-0.3, last + 0.3);
                self.sel = self.carousel.round().clamp(0.0, last) as usize;
            }
            Some(_) => {}
            None => self.carousel += (self.sel as f32 - self.carousel) * ease,
        }
        if let Some((per_row, on_screen, row_h)) = rows(mode) {
            let most = n.div_ceil(per_row).saturating_sub(on_screen) as f32;
            match (&self.drag, self.fling) {
                (Some(d), _) if d.moved().1.abs() > TAP_SLOP => {
                    // Past either end, the finger moves it less and less.
                    let to = d.scroll - d.moved().1 / row_h;
                    let screen = on_screen as f32;
                    self.scroll = if to < 0.0 {
                        -rubber(-to, screen)
                    } else if to > most {
                        most + rubber(to - most, screen)
                    } else {
                        to
                    };
                    self.keep_in_view(mode);
                }
                (Some(_), _) => {}
                // Flicked: coasting, slowing, the selection kept on screen as it goes.
                (None, Some(speed)) => {
                    (self.scroll, self.fling) = coast(self.scroll, speed, dt, most, row_h);
                    self.keep_in_view(mode);
                }
                // Scroll just far enough to keep the selected row in view; in the list, with
                // a row either side of it, so that what is next shows before it is reached.
                // Back from past either end, too.
                (None, None) => {
                    let row = (self.sel / per_row) as f32;
                    let margin = if per_row == 1 { 1.0 } else { 0.0 };
                    let to = self
                        .scroll
                        .clamp(row + margin - (on_screen - 1) as f32, row - margin)
                        .clamp(0.0, most);
                    self.scroll += (to - self.scroll) * ease;
                }
            }
        }
        pull
    }

    /// The cart going into the slot, or coming out: its pull, and whether the insert is done
    /// (its click played out, the screens faded).
    fn slot(&mut self, dt: f32, sound: &mut Sound, pull: f32) -> (f32, bool) {
        let mut pull = pull;
        let mut done = false;
        if let Some(t) = self.insert.as_mut() {
            *t += dt;
            // Slide in and push past the seat; the cart springs back as the latch clicks.
            let latch = ((*t - INSERT_SECS - sound::LATCH_GAP) / SPRING_SECS).clamp(0.0, 1.0);
            pull = (*t / INSERT_SECS).min(1.0).powi(2) * (SEAT + PRESS) - PRESS * latch;
            // Queue the clip as the cart starts to move, behind enough silence that its first
            // click lands as the cart bottoms out. The silence gives the sound card time to wake.
            if !self.clicked {
                crate::mark("insert started");
                sound.play_after(INSERT_SECS - sound::CLICK_LEAD, sound::INSERT);
                self.clicked = true;
            }
            // Leaving closes the audio device, so let the latch click finish first.
            done = *t > INSERT_SECS + sound::LATCH_GAP + FADE_SECS && sound.drained();
        }
        if let Some(t) = self.eject {
            if !self.popped {
                sound.play_after(sound::EJECT_WARMUP, sound::EJECT);
                self.popped = true;
            }
            // Pushed in on the first click, then thrown clear on the second.
            let pop = sound::CLICK_LEAD + sound::POP_GAP;
            let push = ((t - sound::CLICK_LEAD) / SPRING_SECS).clamp(0.0, 1.0);
            let out = ((t - pop) / EJECT_SECS).clamp(0.0, 1.0);
            pull = (SEAT + PRESS * push) * (1.0 - out).powi(2);
            self.eject = (t < pop + EJECT_SECS).then_some(t + dt.max(0.001));
        }
        (pull, done)
    }

    /// How far the screens have faded to black as the game starts.
    fn fade(&self) -> f32 {
        self.insert.map_or(0.0, |t| {
            ((t - INSERT_SECS - sound::LATCH_GAP) / FADE_SECS).clamp(0.0, 1.0)
        })
    }

    /// The artwork to ask for, most wanted first, and whose full-size artwork to keep: the
    /// selection, what else is on screen, then what is next to it. Every small cover follows,
    /// nearest first, so that scrolling finds them already made. Places in `order`; those past
    /// either end come to nothing.
    fn wanted(&self, mode: Layout) -> (Vec<Job>, HashSet<usize>) {
        let sel = self.sel;
        let near: Vec<usize> = match rows(mode) {
            Some((per_row, on_screen, _)) => {
                let top = self.scroll.max(0.0) as usize * per_row;
                (top..top + per_row * (on_screen + 1)).collect()
            }
            None => (1..=3)
                .flat_map(|d| [sel.wrapping_sub(d), sel + d])
                .collect(),
        };
        let next: Vec<usize> = match rows(mode) {
            // The games the top screen shows next: either side and a row up or down in the
            // grid, the next two either way in the list.
            Some((per_row, ..)) => vec![
                sel.wrapping_sub(1),
                sel + 1,
                sel.wrapping_sub(per_row.max(2)),
                sel + per_row.max(2),
            ],
            None => (4..=8)
                .flat_map(|d| [sel.wrapping_sub(d), sel + d])
                .collect(),
        };
        let grid = rows(mode).is_some();
        let kind = |i: &usize| if grid { Job::Thumb(*i) } else { Job::Art(*i) };
        let wants = [Job::Art(sel), Job::Thumb(sel)]
            .into_iter()
            .chain(near.iter().map(kind))
            .chain(next.iter().map(|&i| Job::Art(i)))
            .chain(
                (1..=THUMBS / 2)
                    .flat_map(|d| [Job::Thumb(sel.wrapping_sub(d)), Job::Thumb(sel + d)]),
            )
            .collect();
        let hold = if grid {
            next.into_iter().chain([sel]).collect()
        } else {
            near.into_iter().chain(next).chain([sel]).collect()
        };
        (wants, hold)
    }
}

/// The line at the top of the touch screen: where the selection is, and what is filtering or
/// sorting the list.
fn heading(state: &State, settings: &Settings) -> String {
    let n = state.order.len();
    let place = (n > 0).then(|| format!("{} / {n}", state.sel + 1));
    let (show, sort) = (settings.show(settings.page), settings.sort(settings.page));
    let notes = [
        (show == Show::Favourites).then_some("Favourites"),
        (sort != Sort::Name).then_some(sort.label()),
    ];
    let parts: Vec<String> = place
        .into_iter()
        .chain(notes.into_iter().flatten().map(str::to_owned))
        .collect();
    parts.join("   \u{b7}   ")
}

fn show(
    mut ui: Ui,
    screens: Frame,
    shelf: &Shelf,
    games: &mut Vec<Game>,
    back: Option<Back>,
    stats: &mut Stats,
    settings: &mut Settings,
) -> Result<Outcome, String> {
    let Shelf {
        emulators,
        opts,
        power,
        ..
    } = *shelf;
    let Frame {
        sdl,
        dual,
        headless,
    } = screens;
    let bot = ui.bot;

    // Controllers are looked for once the shelf is on screen: SDL opens every input device to
    // ask what it is, and on nibOS each close costs an RCU grace period (~0.5 s in all).
    let mut pads: Option<Pads> = None;
    let mut sound = Sound::new(settings.sounds());
    let mut panel = if dual { Panel::open() } else { None };
    crate::mark("touch panel ready");

    let mut state = State::new(games, stats, settings, back, &shelf.opts.searched, shelf.ra);
    let mut reader = Reader::default();
    let mut events = sdl.event_pump()?;
    crate::mark("shelf set up");
    let mut pointers = Vec::new();
    let mut tick = Instant::now();
    let mut frame = 0u32;
    let mut fps_from = Instant::now();
    // Since someone last touched anything, and the screensaver while it is on.
    let mut idle = Instant::now();
    let mut saver: Option<Saver> = None;
    let registry = crate::emulator::signature(emulators);
    let mut registry_at = Instant::now();
    // What the watch on the games' folders found, until it can be taken in; and, after Rescan
    // games in the menu, the games that came and went since.
    let mut found: Vec<Found> = Vec::new();
    let mut asked: Option<(usize, usize)> = None;
    // Whether the last frame drawn had all it wanted, and what its status bar showed: once
    // nothing has been touched for a while, nothing moves and nothing is coming in, frames are
    // only drawn when that changes (the clock's minute, the battery, the radios).
    let mut was_settled = false;
    let mut drawn_status = None;
    loop {
        // A game chosen (or one put away carrying on): this frame is drawn, then the shelf
        // goes.
        let mut leaving = None;
        // Packages install their own descriptions. Notice additions, removals, changed options,
        // and binaries/cores becoming available, including while the screensaver is up.
        if opts.shot.is_none() && registry_at.elapsed() >= Duration::from_secs(2) {
            registry_at = Instant::now();
            let current = crate::emulator::Emulators::load(shelf.emulator_dirs);
            if crate::emulator::signature(&current) != registry {
                leaving = Some(Outcome::Refresh(state.order.get(state.sel).copied()));
            }
        }
        let gap = tick.elapsed();
        let dt = gap.as_secs_f32().min(0.05);
        tick = Instant::now();

        // The screensaver: nothing else is worked out or drawn while it is on. Anything wakes
        // it (the system having slept too), and does nothing else: the first press is its.
        if leaving.is_some() {
            saver = None;
        }
        if let Some(on) = saver.as_mut() {
            let mut wake = gap >= SLEPT;
            for event in events.poll_iter() {
                if let Event::Quit { .. } = event {
                    return Ok(Outcome::Quit);
                }
                if let Some(pads) = pads.as_mut() {
                    pads.event(&event);
                }
                wake |= woken_by(&event);
            }
            if let Some(panel) = panel.as_mut() {
                panel.poll(&mut pointers);
                wake |= pointers.iter().any(|p| matches!(p, Pointer::Down(..)));
                pointers.clear();
            }
            if power.take() {
                wake = true;
                state.power = Some(PowerMenu::open(None));
            }
            if wake {
                saver = None;
                crate::mark("screensaver off");
                idle = Instant::now();
                continue;
            }
            // A frame only when the logo is due to move: in between, nothing is drawn or
            // presented, and the GPU sleeps.
            if on.due() {
                let now = bouncer::screens(&on.bouncer);
                ui.draw_saver(&on.bouncer, [now[0] || on.shown[0], now[1] || on.shown[1]])?;
                on.shown = now;
                on.drawn = Some(Instant::now());
            }
            std::thread::sleep(REST_POLL);
            continue;
        }
        // One scripted key a frame, as a person would press them.
        if let Some(name) = opts.press.get(frame as usize) {
            let (down, up) = input::scripted(name);
            sdl.event()?.push_event(down)?;
            sdl.event()?.push_event(up)?;
        }
        // Games copied onto a card or deleted from it: taken in between frames, while no cart
        // is moving and no finger is down.
        found.extend(shelf.rescan.found());
        if !found.is_empty() && !state.busy() && state.drag.is_none() {
            let mut changed = false;
            for f in found.drain(..) {
                match f {
                    Found::Games(console, mut list) => {
                        crate::rom::name_from_lists(&mut list, shelf.emulators);
                        let (came, went) = library::merge(games, console, list);
                        if came + went > 0 {
                            eprintln!("dslot: {}: {came} new, {went} gone", console.label());
                            changed = true;
                        }
                        if let Some((c, w)) = asked.as_mut() {
                            (*c, *w) = (*c + came, *w + went);
                        }
                    }
                    Found::Done => {
                        if let (Some(counts), Some(open)) = (asked.take(), state.menu.as_mut()) {
                            open.rescanned(counts);
                        }
                    }
                }
            }
            if changed {
                state.rescanned(games, stats, settings);
                state.repage(games, &opts.searched, settings);
            }
        }
        let games: &[Game] = games;
        if let Some(to) = state.turn_page.take() {
            state.turn_to(games, to, stats, settings);
        }
        let console = settings.page;
        let n = state.order.len();
        let geometry = cart_geometry(console);
        // Games on this page before the favourites filter: none at all is its own screen.
        let on_page = games
            .iter()
            .filter(|g| g.console == console && !g.gone)
            .count();
        let mode = mode(console, settings);
        let (first, next) = repeat_times(settings);
        // Each console's page may want its boxes in another case, or its carts another shape.
        if ui.looks != settings.look(console) {
            ui.looks = settings.look(console);
            ui.forget_art();
        }
        let busy = state.busy();
        // The heading, and the tab strip beside it, scrolled to this page's tab.
        let title = heading(&state, settings);
        let strip = state.strip(ui.heading_width(&title), console, dt);
        let blades = Blades::DEEP;

        pointers.clear();
        if dual
            && panel.is_none()
            && frame.is_multiple_of(PANEL_RETRY)
            && (1..PANEL_RETRY_FOR).contains(&frame)
        {
            panel = Panel::open();
        }
        if let Some(panel) = panel.as_mut() {
            panel.poll(&mut pointers);
            pointers
                .iter_mut()
                .for_each(|ptr| *ptr = ptr.scaled(SW, SH));
        }
        if !pointers.is_empty() {
            idle = Instant::now();
        }

        // The power menu, asked for (a long press of power): over whatever is going on.
        if power.take() && state.power.is_none() {
            crate::mark("power menu");
            (state.power, state.menu, state.insert, state.drag) =
                (Some(PowerMenu::open(None)), None, None, None);
            state.repeat.stop();
        }

        // This frame's buttons: the power menu's, the menu's, or the shelf's.
        let (mut power_moved, mut power_pick, mut power_cancel) = (0, false, false);
        let mut menu_input = menu::Input::default();
        let (mut fave, mut flip) = (false, None);
        // The system picker: whether it goes to its blade, or closes.
        let (mut pick_go, mut pick_close) = (false, false);
        // With the panel read directly, SDL's copies of the same touches are ignored.
        let mouse = panel.is_none().then_some(bot);
        for event in events.poll_iter() {
            if let Some(pads) = pads.as_mut() {
                pads.event(&event);
            }
            if woken_by(&event) {
                idle = Instant::now();
            }
            match reader.read(event, mouse) {
                // Letting go of a direction stops its repeat, whatever else is going on (and of
                // left or right, a slider's move).
                Some(Input::Release(key @ (Key::Up | Key::Down | Key::Left | Key::Right))) => {
                    state.repeat.release(key);
                    menu_input.released |= matches!(key, Key::Left | Key::Right);
                }
                None | Some(Input::Release(_)) => {}
                Some(Input::Quit) => return Ok(Outcome::Quit),
                Some(Input::Press(key)) if state.power.is_some() => match key {
                    Key::Up | Key::Down => power_moved += state.hold(key),
                    Key::A => power_pick = true,
                    Key::B | Key::X | Key::Start => power_cancel = true,
                    _ => {}
                },
                Some(Input::Pointer(p)) if state.power.is_some() => pointers.push(p),
                // The notice of a game that came straight back: Y may send a bug report; any
                // other button or a touch closes it.
                Some(Input::Press(key)) if state.notice.is_some() => {
                    if !state.notice.as_mut().is_some_and(|n| n.press(Some(key))) {
                        state.notice = None;
                    }
                    state.repeat.stop();
                }
                Some(Input::Pointer(Pointer::Down(..))) if state.notice.is_some() => {
                    if !state.notice.as_mut().is_some_and(|n| n.press(None)) {
                        state.notice = None;
                    }
                    state.repeat.stop();
                }
                Some(Input::Pointer(_)) if state.notice.is_some() => {}
                Some(Input::Pointer(_) | Input::Press(_)) if busy => {}
                Some(Input::Pointer(p)) => pointers.push(p),
                Some(Input::Press(key)) if state.picker.is_some() => match key {
                    Key::Up | Key::Down | Key::Left | Key::Right => {
                        let d = state.hold(key);
                        let n = state.pages.len();
                        if let Some(p) = state.picker.as_mut() {
                            p.step(d, n);
                        }
                    }
                    Key::A => pick_go = true,
                    Key::B | Key::Select | Key::X | Key::Start => pick_close = true,
                    Key::Y | Key::L | Key::R => {}
                },
                Some(Input::Press(key)) if state.menu.is_some() => match key {
                    // While typing, the keyboard has them all.
                    _ if state.menu.as_ref().is_some_and(|m| m.typing()) => {
                        menu_input.keys.extend(keyboard_key(key))
                    }
                    Key::Up | Key::Down => menu_input.moved += state.hold(key),
                    // Held, as for a slider, it keeps going.
                    Key::Left | Key::Right => menu_input.turn += state.hold(key),
                    Key::A => (menu_input.turn, menu_input.pressed) = (menu_input.turn + 1, true),
                    Key::Y => menu_input.reset = true,
                    Key::L => menu_input.page -= 1,
                    Key::R => menu_input.page += 1,
                    Key::B | Key::X | Key::Start => menu_input.close = true,
                    Key::Select => {}
                },
                // Select alone: the system picker, on this page's blade.
                Some(Input::Press(Key::Select)) => {
                    let at = state.pages.iter().position(|&p| p == console).unwrap_or(0);
                    state.picker = Some(Picker::on(at));
                    state.repeat.stop();
                }
                Some(Input::Press(Key::X)) if !games.is_empty() => {
                    state.menu = Some(Menu::open(state.order.get(state.sel).is_some(), shelf.ra))
                }
                Some(Input::Press(Key::L)) => flip = Some(state.beside(console, -1)),
                Some(Input::Press(Key::R)) => flip = Some(state.beside(console, 1)),
                Some(Input::Press(_)) if n == 0 => {}
                Some(Input::Press(key)) => fave |= state.press(games, key, mode),
            }
        }
        // A direction held: once more, to whatever is showing.
        state.repeat.set_times(first, next);
        if let Some((key, d)) = state.repeat_due() {
            let vertical = matches!(key, Key::Up | Key::Down);
            if state.power.is_some() {
                power_moved += if vertical { d } else { 0 };
            } else if state.menu.is_some() {
                if vertical {
                    menu_input.moved += d;
                } else {
                    menu_input.turn += d;
                }
            } else if let Some(p) = state.picker.as_mut() {
                p.step(d, state.pages.len());
            } else if vertical {
                state.sel = state.vertical(games, d, mode);
            } else {
                state.sel = state.horizontal(games, d, mode);
            }
        }
        if let Some(open) = state.power.as_mut() {
            open.moved(power_moved);
            for p in pointers.drain(..) {
                if let Pointer::Down(x, y) = p {
                    if let Some(r) = open.hit(x, y) {
                        (open.row, power_pick) = (r, true);
                    }
                }
            }
            let choice = if power_cancel {
                Some(Choice::Cancel)
            } else {
                power_pick.then(|| open.chosen())
            };
            match choice {
                Some(Choice::PowerOff) => return Ok(Outcome::PowerOff),
                Some(Choice::Restart) => return Ok(Outcome::Restart),
                Some(Choice::Sleep) => {
                    crate::mark("sleeping");
                    nibkit::device::sleep();
                    crate::mark("awake");
                    // The press that woke the device is no request for the menu.
                    power.take();
                }
                Some(Choice::Cancel) | None => {}
            }
            // Cancel, or awake again: the game put away for the menu carries on.
            if matches!(choice, Some(Choice::Cancel | Choice::Sleep)) {
                if let Some(game) = open.resume {
                    leaving = Some(Outcome::Resume(game));
                }
                state.power = None;
            }
        }

        let mut relist = false;
        let current = state.order.get(state.sel).map(|&g| &games[g]);
        if let (true, Some(game)) = (fave, current) {
            stats.toggle_fave(&game.path);
            relist = true;
        }
        if let Some(open) = state.menu.as_mut() {
            state.drag = None;
            let taps = pointers.drain(..).filter_map(|p| match p {
                Pointer::Down(x, y) => Some((x, y)),
                _ => None,
            });
            menu_input.taps = taps.collect();
            let mut cx = menu::Context {
                stats: &mut *stats,
                settings: &mut *settings,
                emulators,
                console,
                game: current,
                // The selected game's achievements, for the Game page's row and its list.
                cheevos: current.and_then(|g| ui.cheevos_set(&g.path)),
            };
            let done = open.update(&menu_input, &mut cx);
            if done.account {
                // Signed in or out: what was found was the last player's.
                ui.forget_cheevos();
            }
            if done.power_off {
                return Ok(Outcome::PowerOff);
            }
            if done.restart {
                return Ok(Outcome::Restart);
            }
            if done.rescan {
                // Rescan games also makes the folders a card is missing (folders.rs).
                crate::folders::make_soon(&crate::platform::rom_roots(), shelf.emulators);
                shelf.rescan.now();
                asked = Some((0, 0));
            }
            if done.changed {
                settings.save();
                sound.on = settings.sounds();
                relist = true;
            }
            if done.closed {
                state.menu = None;
                // A resume point may have been cleared there.
                ui.forget_resumes();
            }
        }
        // The system picker: A, or a tap on a blade, turns to that page; B, Select, or a tap off
        // the blades closes it. A finger drags the stack, which settles on the nearest blade
        // when it lifts; otherwise it glides to the blade chosen. Nothing else takes touches.
        if let Some(mut p) = state.picker {
            let n = state.pages.len();
            let last = n.saturating_sub(1) as f32;
            for ptr in pointers.drain(..) {
                match ptr {
                    Pointer::Down(_, y) => p.drag = Some((y, p.pos, y)),
                    Pointer::Move(_, y) => {
                        if let Some(d) = p.drag.as_mut() {
                            d.2 = y;
                        }
                    }
                    Pointer::Up => {
                        let Some((from, _, to)) = p.drag.take() else {
                            continue;
                        };
                        if (to - from).abs() < TAP_SLOP {
                            match blades.hit(from, n, p.pos) {
                                Some(i) => (p.at, pick_go) = (i, true),
                                None => pick_close = true,
                            }
                        } else {
                            p.at = p.pos.round().clamp(0.0, last) as usize;
                        }
                    }
                }
            }
            match p.drag {
                Some((from, pos, to)) if (to - from).abs() >= TAP_SLOP => {
                    p.pos = (pos - (to - from) / blades.drag_step()).clamp(-0.4, last + 0.4);
                    p.at = p.pos.round().clamp(0.0, last) as usize;
                }
                _ => p.pos += (p.at as f32 - p.pos) * (1.0 - (-dt * FOLLOW).exp()),
            }
            if pick_go {
                flip = state.pages.get(p.at).copied();
            }
            state.picker = (!pick_go && !pick_close).then_some(p);
            if state.picker.is_none() {
                state.repeat.stop();
            }
        }
        // A tap on another page's tab turns to it, and one on a faded edge of the strip scrolls
        // it; touches on the strip go no further.
        if state.menu.is_none() && !busy {
            let pages = state.pages.clone();
            let step = strip.width * 0.75;
            pointers.retain(|ptr| match *ptr {
                Pointer::Down(x, y) => match strip.hit(x, y) {
                    Some(StripHit::Tab(i)) => {
                        flip = pages.get(i).copied();
                        false
                    }
                    Some(StripHit::Left) => {
                        state.strip.1 = (state.strip.1 - step).max(0.0);
                        false
                    }
                    Some(StripHit::Right) => {
                        state.strip.1 = (state.strip.1 + step).min(strip.most());
                        false
                    }
                    None => true,
                },
                _ => true,
            });
        }
        // Turned at the top of the next frame, before anything is worked out for this page. A
        // page chosen is the one to stay on.
        if let Some(p) = flip.filter(|&p| p != settings.page && !busy) {
            state.turn_page = Some(p);
            state.away_from = None;
        }
        if relist {
            state.relist(games, stats, settings);
            state.repage(games, &opts.searched, settings);
            pointers.clear();
        }
        // A tap on the status bar's Wi-Fi or Bluetooth icon: the menu, on that page.
        if state.menu.is_none() && state.picker.is_none() && state.power.is_none() {
            let hit = pointers.iter().find_map(|p| match *p {
                Pointer::Down(x, y) => ui.radio_hit(x, y),
                _ => None,
            });
            if let Some(bluetooth) = hit {
                let game = state.order.get(state.sel).is_some();
                state.menu = Some(Menu::open_radio(game, bluetooth, shelf.ra));
                pointers.clear();
            }
        }
        if !busy && n > 0 {
            state.touch(&pointers, mode, geometry);
        }

        // Left alone long enough, on the shelf or its menu: the screensaver. Not over the power
        // menu, nor while a cart moves, nor for a picture or with nothing to show it on.
        let saver_due = settings
            .saver()
            .after()
            .is_some_and(|after| idle.elapsed() >= after);
        if saver_due
            && leaving.is_none()
            && state.power.is_none()
            && !busy
            && !headless
            && opts.shot.is_none()
        {
            saver = Some(Saver::start(settings.saver_look()));
            state.repeat.stop();
            continue;
        }

        let pull = state.follow(dt, mode, geometry.2);
        let (pull, inserted) = state.slot(dt, &mut sound, pull);
        if inserted {
            leaving = Some(Outcome::Launch(state.order[state.sel]));
        }
        sound.tick(dt);

        // A bug report on its way from the notice: its outcome, when it comes.
        if let Some(n) = state.notice.as_mut() {
            n.poll();
        }
        // A notice nibOS left for the shelf, once nothing else is over it: now, and when one
        // appears while the shelf is up.
        if state.notice.is_none() && state.power.is_none() && state.menu.is_none() {
            let next = opts
                .notices
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .next();
            if let Some(text) = next {
                state.notice = Some(Notice::from_system(text));
                // Drawn now, whatever was resting.
                idle = Instant::now();
            }
        }
        // Resting: the screens stay as they are, and the loop only looks for input, a few
        // times a second, until something changes. A held direction, a finger down, a cart
        // moving, games found, artwork coming in or a report on its way keep it drawing.
        let resting = !headless
            && !state.notice.as_ref().is_some_and(Notice::sending)
            && opts.shot.is_none()
            && idle.elapsed() >= REST_AFTER
            && was_settled
            && !busy
            && leaving.is_none()
            && state.drag.is_none()
            && state.repeat.held().is_none()
            && found.is_empty()
            && !ui.waiting();
        let status = ui.status();
        if resting && status.is_some() && status == drawn_status {
            std::thread::sleep(REST_POLL);
            continue;
        }
        drawn_status = status;

        ui.clear();
        let settled;
        // With the system picker open, its blades: each page's game whose art it shows. Their
        // art is asked for along with the page's own, the chosen blade's first, and kept.
        let shown: Vec<Blade> = match state.picker {
            Some(_) => state
                .pages
                .iter()
                .map(|&c| blade(games, stats, c))
                .collect(),
            None => Vec::new(),
        };
        let extra: Vec<usize> = shown.iter().filter_map(|b| b.game).collect();
        let mut list = state.order.clone();
        list.extend(&extra);
        let (first, at) = (state.order.len(), state.picker.map_or(0, |p| p.at));
        let blade_wants = || {
            let mut places: Vec<usize> = (0..extra.len()).collect();
            places.sort_by_key(|&k| (k as i32 - at as i32).abs());
            places.into_iter().map(move |k| Job::Art(first + k))
        };
        if on_page == 0 {
            ui.empty(&opts.searched[console as usize], console)?;
            ui.tab(console, &state.pages, &strip)?;
            let hold = (first..list.len()).collect();
            settled = ui.load(games, &list, blade_wants(), &hold, 0)?;
            was_settled = settled;
        } else {
            let (wants, mut hold) = state.wanted(mode);
            hold.extend(first..list.len());
            let wants = blade_wants().chain(wants);
            settled = ui.load(games, &list, wants, &hold, state.sel)?;
            was_settled = settled;
            if frame == 0 {
                crate::mark("artwork asked for");
            }
            let fade = state.fade();
            let current = state.order.get(state.sel).copied();
            match current {
                Some(g) => {
                    let resume = resume_of(shelf, settings, &games[g]);
                    let browsing = state.menu.as_ref().and_then(|m| m.cheevos_list);
                    ui.draw_top(games, g, stats, resume.as_ref(), fade, browsing)?
                }
                None => {
                    let top = ui.top;
                    ui.text(
                        top,
                        "No favourites yet",
                        (48.0, true),
                        (SW / 2.0, 320.0),
                        0.5,
                        WHITE,
                    )?;
                    ui.text(
                        top,
                        "Show all games again from the menu.",
                        (24.0, false),
                        (SW / 2.0, 392.0),
                        0.5,
                        DIM,
                    )?;
                }
            }
            let heading = heading(&state, settings);
            let page = Page {
                games,
                list: &state.order,
                sel: state.sel,
                console,
                heading: &heading,
                strip,
                fade,
            };
            match &state.menu {
                Some(open) => {
                    let game = current.map(|g| &games[g]);
                    let cx = menu::Context {
                        stats: &mut *stats,
                        settings: &mut *settings,
                        emulators,
                        console,
                        game,
                        cheevos: game.and_then(|g| ui.cheevos_set(&g.path)),
                    };
                    ui.draw_menu(&open.shown(&cx), console)?;
                }
                None => match mode {
                    Layout::Shelf => ui.draw_carts(&page, state.carousel, pull)?,
                    Layout::Grid => ui.draw_grid(&page, state.scroll, stats)?,
                    Layout::List | Layout::ListCovers => {
                        ui.draw_list(&page, state.scroll, stats, mode == Layout::ListCovers)?
                    }
                },
            }
            if state.menu.is_none() {
                ui.tab(console, &state.pages, &strip)?;
            }
        }
        if let Some(p) = state.picker {
            soft(
                "the system picker",
                ui.draw_picker(&shown, blades, p.pos, p.at),
            );
        }
        // A system's card on the top screen: the chosen system's in the picker, or the console's
        // while its page of the menu is open.
        let card_for = match (&state.picker, &state.menu) {
            (Some(p), _) => state.pages.get(p.at).copied(),
            (None, Some(open)) if open.page == menu::Page::Console => Some(console),
            _ => None,
        };
        if let Some(c) = card_for {
            // The Apps page's card is about the device (device.rs), made as it is shown.
            if c == Console::Apps {
                let about = crate::device::about(games, stats);
                soft(
                    "the device's card",
                    ui.draw_about(&about, &yours(games, stats, c)),
                );
            } else if let Some(card) = crate::systems::card(c) {
                soft(
                    "a system card",
                    ui.draw_card(c, card, &yours(games, stats, c)),
                );
            }
        }
        if let Some(notice) = &state.notice {
            soft("the stopped notice", ui.draw_notice(&notice.shown()));
        }
        if let Some(open) = &state.power {
            let labels: Vec<&str> = open.choices.iter().map(|c| c.label()).collect();
            ui.draw_power(&labels, open.row)?;
        }
        if frame == 0 {
            crate::mark("first frame drawn");
        }
        // A port plays on the top screen alone: the bottom one says how to get out of it.
        if let Some(Outcome::Launch(g) | Outcome::Resume(g)) = leaving {
            if games[g].console == Console::Ports {
                ui.draw_port_help(games[g].name())?;
            }
        }
        // Leaving for a game: this frame is what DSlot keeps on screen while it goes.
        if leaving.is_some() && !headless {
            let frame = ui.read_screens();
            // Into DSlot's own buffers now: when SDL's windows go, SDL puts back what was on
            // screen when it started, which is them.
            if let (Some(out), Some([top, bot])) = (
                opts.outputs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_mut(),
                frame.as_ref(),
            ) {
                out.paint(&[top, bot]);
            }
            *opts.last_frame.lock().unwrap_or_else(|e| e.into_inner()) = frame;
        }
        // A picture asked for (`--shot`): this frame, read before it is presented (after, what
        // is left to read is undefined), once the presses are done and the artwork is in.
        if let Some(path) = &opts.shot {
            let due = opts.press.len() + if opts.press.is_empty() { 2 } else { 40 };
            if frame as usize + 1 >= due && (settled || on_page == 0) {
                let (w, h, px) = ui.read_pixels()?;
                save_png(path, w, h, &px)?;
                return Ok(Outcome::Quit);
            }
        }
        ui.present();
        if let Some(outcome) = leaving {
            return Ok(outcome);
        }
        if frame < 2 {
            crate::mark(if frame == 0 {
                "first frame shown"
            } else {
                "second frame shown"
            });
        }
        if frame == 1 {
            platform::first_frame_shown();
            pads = Some(Pads::open(sdl.game_controller()?)?);
            crate::mark("controllers ready");
            // Also not wanted for the first frame, and slow to start cold (~0.7 s on nibOS).
            sound.set_audio(sdl.audio().ok());
            crate::mark("audio ready");
            crate::gfx::warm_fallback();
        }
        frame += 1;
        if frame.is_multiple_of(300) && std::env::var_os("DSLOT_FPS").is_some() {
            eprintln!("dslot: {:.1} fps", 300.0 / fps_from.elapsed().as_secs_f32());
            fps_from = Instant::now();
        }

        if headless {
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flick_is_as_fast_as_the_finger_was_as_it_lifted() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        // 600 units up the screen in the last 60 ms: 10 000 units a second.
        let flick = [
            (at(0), 900.0),
            (at(100), 800.0),
            (at(140), 500.0),
            (at(160), 200.0),
        ];
        let speed = release_speed(&flick, at(160));
        assert!((-10_100.0..-9_900.0).contains(&speed), "{speed}");
        // The same, but held still for 100 ms before lifting: nothing.
        let held = [(at(0), 800.0), (at(60), 200.0), (at(160), 200.0)];
        assert_eq!(release_speed(&held, at(160)), 0.0);
        assert_eq!(release_speed(&[], at(160)), 0.0);
    }

    #[test]
    fn a_flicked_list_slows_to_a_stop() {
        let (mut scroll, mut speed) = (10.0, Some(40.0));
        let mut frames = 0;
        while let Some(v) = speed {
            (scroll, speed) = coast(scroll, v, 1.0 / 60.0, 1000.0, 50.0);
            frames += 1;
            assert!(frames < 60 * 10, "never stopped");
        }
        // About speed / decay rows on, in a few seconds.
        assert!((18.0..21.0).contains(&(scroll - 10.0)), "{scroll}");
        assert!(frames > 60 * 2, "{frames}");
        // Into either end: it stops soon after, a little past it, to spring back.
        let (mut scroll, mut speed) = (2.0, Some(-40.0));
        while let Some(v) = speed {
            (scroll, speed) = coast(scroll, v, 1.0 / 60.0, 100.0, 50.0);
        }
        assert!((-0.5..0.0).contains(&scroll), "{scroll}");
    }

    #[test]
    fn past_either_end_a_finger_moves_the_list_less_and_less() {
        assert_eq!(rubber(0.0, 11.0), 0.0);
        let (one, two) = (rubber(1.0, 11.0), rubber(2.0, 11.0));
        assert!(one < 1.0 && two - one < one && two < 2.0);
        assert!(rubber(1000.0, 11.0) < 11.0);
    }
}
