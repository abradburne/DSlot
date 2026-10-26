//! The shelf: box and details on the top screen, cart carousel and slot on the touch screen.
//!
//! - `screens`: SDL, the windows and renderers, one per panel or one spanning both.
//! - `shelf`: the shelf itself, a frame at a time: input, motion, artwork, drawing.
//! - `input`: SDL's events as the DS+'s buttons and the touch panel's pointers.
//! - `draw`: the textures and how each part of the screens is drawn.
//! - `menu`: the menu's rows on the touch screen.
//! - `layout`: where things are on the screens, in screen units.
//! - `skeleton`: the shelf's layout with nothing in it, for the first moment at power-on.
//! - `sound`: the insert and eject clicks.
//! - `bouncer`: the screensaver's logo, bouncing round both screens.

mod bouncer;
mod draw;
mod input;
mod layout;
mod menu;
mod power;
mod radios;
mod screens;
mod shelf;
mod skeleton;
mod sound;

pub use screens::{open, Screens};
pub use shelf::run;
pub use skeleton::skeleton;

use crate::art::Shell;
use crate::emulator::Emulators;
use crate::gfx::Fonts;
use crate::kms::Kms;
use crate::rescan::Rescan;
use std::path::PathBuf;
use std::sync::Mutex;

/// How the shelf ends.
pub enum Outcome {
    /// The game at this index was pushed into the slot.
    Launch(usize),
    /// The game at this index, put away for the power menu, carries on (Cancel, or a wake).
    Resume(usize),
    /// Installed emulator descriptions changed: the pages and menus are made again.
    Refresh(Option<usize>),
    Quit,
    Restart,
    PowerOff,
}

/// The game the shelf comes back from: an index into the games, and whether it was put away
/// for the power menu (which then opens at once) rather than left.
#[derive(Clone)]
pub struct Back {
    pub game: usize,
    pub put_away: bool,
    /// Why it came straight back, if it did: shown until a button is pressed.
    pub notice: Option<String>,
}

/// How DSlot was asked to run.
pub struct Opts {
    /// The touch panel's screen on the other panel.
    pub swap: bool,
    /// Draw one frame to this PNG and quit.
    pub shot: Option<PathBuf>,
    /// Keys pressed before the shot is taken, by SDL name.
    pub press: Vec<String>,
    /// The folders each console's games were looked for in, by `Console as usize`: none for a
    /// console with no page.
    pub searched: [Vec<PathBuf>; crate::rom::Console::COUNT],
    /// Folder for grid covers already scaled, so they are only made once.
    pub cache: PathBuf,
    /// DSlot's own hold on the screens (`kms`), for its whole life: the skeleton at start-up,
    /// the shelf's last frame between it and a game.
    pub outputs: Mutex<Option<Kms>>,
    /// The shelf's last frame, top screen and bottom, as it left for a game.
    pub last_frame: Mutex<Option<[crate::gfx::Img; 2]>>,
    /// Notices nibOS leaves for the shelf (notices.rs), and which have been shown this run.
    pub notices: Mutex<crate::notices::Notices>,
}

/// What the shelf works with, the same every time it comes up. (The games are passed beside
/// it: games copied onto a card while the shelf is up join them.)
#[derive(Clone, Copy)]
pub struct Shelf<'a> {
    pub emulators: &'a Emulators,
    pub emulator_dirs: &'a [PathBuf],
    pub fonts: &'a Fonts,
    pub shell: &'a Shell,
    pub opts: &'a Opts,
    /// Requests for the power menu (a long press of power on nibOS).
    pub power: &'a nibkit::power::Requests,
    /// Games copied onto the cards or deleted from them, noticed as the shelf is up.
    pub rescan: &'a Rescan,
}
