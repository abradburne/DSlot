//! The game page (X): the selected game's own page, over both screens. The top screen turns its
//! box round to the back (back.rs). The touch screen has a row of buttons (Play, Favourite,
//! Rating, Settings: the menu, on the game's Game page) and under them the set's badges in a
//! grid, the ones earned rimmed in gold, the rest dimmed; the badge chosen goes on the back of
//! the box as a sticker. The D-pad moves between them, A presses a button, a tap presses or
//! picks. With no set to show, the grid's place says why, or offers a sign-in (the menu's
//! Achievements page).

use super::back::Mine;
use super::draw::{Ui, DIM, FAINT, GOLD, PANEL, RED, WHITE};
use super::layout::*;
use super::menu::Input;
use crate::achievements::set::{Answer, Set};
use crate::rom::Game;
use crate::stats::Stats;
use sdl2::pixels::Color;

/// A button along the top of the touch screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Play,
    Favourite,
    Rating,
    Settings,
}

const BUTTONS: [Button; 4] = [
    Button::Play,
    Button::Favourite,
    Button::Rating,
    Button::Settings,
];

/// What the D-pad is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Button(Button),
    /// A badge, by its achievement's place in the set.
    Badge(usize),
    /// The sign-in offered in the grid's place.
    SignIn,
}

// The touch screen's layout, in its units: the buttons, then the grid of badges.
const MARGIN: f32 = 40.0;
const BUTTON_Y: f32 = 34.0;
const BUTTON_H: f32 = 80.0;
const BUTTON_GAP: f32 = 16.0;
const HEADING_Y: f32 = 142.0;
const GRID_Y: f32 = 186.0;
const BADGE: f32 = 72.0;
const PITCH: f32 = 84.0;
const COLS: usize = ((SW - 2.0 * MARGIN + PITCH - BADGE) / PITCH) as usize;
const ROWS: usize = 5;

/// What the page works on, each frame.
pub struct Context<'a> {
    pub stats: &'a mut Stats,
    pub game: &'a Game,
    /// Its index in the shelf's games, for its box.
    pub g: usize,
    /// What the achievements lookup came to (`Ui::cheevos_answer`), and whether it is still
    /// looking.
    pub cheevos: (Option<Answer>, bool),
}

impl Context<'_> {
    fn set(&self) -> Option<&Set> {
        match &self.cheevos.0 {
            Some(Answer::Set(set)) => Some(set),
            _ => None,
        }
    }

    fn badges(&self) -> usize {
        self.set().map_or(0, |s| s.achievements.len())
    }
}

/// What the page did this frame.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Done {
    /// The favourite or the rating changed: the shelf lists the page again.
    pub changed: bool,
    pub closed: bool,
    /// Play: the page closes and the cart goes in.
    pub play: bool,
    /// The menu, over the page: on the game's Game page, or (`sign_in`) on Achievements.
    pub settings: bool,
    pub sign_in: bool,
    /// Y: the game's screenshots, box art and details looked up again (back.rs).
    pub look_again: bool,
}

/// The open game page.
pub struct GamePage {
    pub focus: Focus,
    /// The first row of badges shown.
    first: usize,
    /// Whether the game can have achievements here: DSlot keeps a RetroAchievements sign-in
    /// (it has curl) and its system is one the service hashes.
    achievements: bool,
    /// Whether a sign-in was kept when the page opened.
    signed_in: bool,
}

impl GamePage {
    pub fn open(game: &Game, signed_in: bool) -> GamePage {
        GamePage {
            focus: Focus::Button(Button::Play),
            first: 0,
            achievements: crate::achievements::available()
                && crate::achievements::hashed(game.console),
            signed_in,
        }
    }

    /// Whether the grid's place offers a sign-in.
    fn offers_sign_in(&self, cx: &Context) -> bool {
        self.achievements
            && match &cx.cheevos {
                (Some(Answer::Refused), _) => true,
                (Some(Answer::Nothing), false) => !self.signed_in,
                _ => false,
            }
    }

    /// What is under the buttons, for moving down to: the first badge on show, or the sign-in.
    fn below(&self, cx: &Context) -> Option<Focus> {
        if cx.badges() > 0 {
            Some(Focus::Badge((self.first * COLS).min(cx.badges() - 1)))
        } else if self.offers_sign_in(cx) {
            Some(Focus::SignIn)
        } else {
            None
        }
    }

    /// L / R: the button before or after, round the row, as L / R turn the menu's tabs. From
    /// a badge, from the button above it.
    fn flip(&mut self, by: i32) {
        let at = match self.focus {
            Focus::Button(b) => BUTTONS.iter().position(|&x| x == b).unwrap_or(0),
            Focus::Badge(i) => (i % COLS) * BUTTONS.len() / COLS,
            Focus::SignIn => 0,
        };
        let n = BUTTONS.len() as i32;
        self.focus = Focus::Button(BUTTONS[(at as i32 + by).rem_euclid(n) as usize]);
    }

    /// Moves the focus `dx` across and `dy` down.
    fn step(&mut self, dx: i32, dy: i32, cx: &Context) {
        let n = cx.badges();
        self.focus = match self.focus {
            Focus::Button(b) => {
                let at = BUTTONS.iter().position(|&x| x == b).unwrap_or(0) as i32;
                let at = (at + dx).clamp(0, BUTTONS.len() as i32 - 1) as usize;
                match dy {
                    1.. => self.below(cx).unwrap_or(Focus::Button(BUTTONS[at])),
                    _ => Focus::Button(BUTTONS[at]),
                }
            }
            Focus::Badge(i) => {
                let i = (i as i32 + dx).clamp(0, n as i32 - 1);
                let down = i + dy * COLS as i32;
                if down < 0 {
                    // Off the top row: the button above it.
                    let b = (i as usize % COLS) * BUTTONS.len() / COLS;
                    Focus::Button(BUTTONS[b])
                } else if dy > 0 && down >= n as i32 {
                    // Past the last row: the last badge, if that is on a row further down.
                    let last = n - 1;
                    match last / COLS > i as usize / COLS {
                        true => Focus::Badge(last),
                        false => Focus::Badge(i as usize),
                    }
                } else {
                    Focus::Badge(down as usize)
                }
            }
            Focus::SignIn if dy < 0 => Focus::Button(Button::Play),
            focus => focus,
        };
        self.follow(cx);
    }

    /// Keeps the focus on something there is, and its badge's row on show.
    fn follow(&mut self, cx: &Context) {
        let n = cx.badges();
        self.focus = match self.focus {
            Focus::Badge(_) if n == 0 => Focus::Button(Button::Play),
            Focus::Badge(i) => Focus::Badge(i.min(n - 1)),
            Focus::SignIn if !self.offers_sign_in(cx) => Focus::Button(Button::Play),
            focus => focus,
        };
        if let Focus::Badge(i) = self.focus {
            let row = i / COLS;
            if row < self.first {
                self.first = row;
            } else if row >= self.first + ROWS {
                self.first = row + 1 - ROWS;
            }
        }
        let rows = n.div_ceil(COLS);
        self.first = self.first.min(rows.saturating_sub(ROWS));
    }

    pub fn update(&mut self, input: &Input, cx: &mut Context) -> Done {
        let mut done = Done::default();
        if input.close {
            done.closed = true;
            return done;
        }
        self.follow(cx);
        let mut pressed = input.pressed;
        for &(x, y) in &input.taps {
            match self.hit(x, y, cx) {
                Some(focus @ Focus::Badge(_)) => self.focus = focus,
                Some(focus) => (self.focus, pressed) = (focus, true),
                None => {}
            }
        }
        if input.turn != 0 || input.moved != 0 {
            self.step(input.turn.signum(), input.moved.signum(), cx);
        }
        if input.page != 0 {
            self.flip(input.page.signum());
        }
        done.look_again = input.reset;
        if pressed {
            let path = &cx.game.path;
            match self.focus {
                Focus::Button(Button::Play) => done.play = true,
                Focus::Button(Button::Favourite) => {
                    cx.stats.toggle_fave(path);
                    done.changed = true;
                }
                Focus::Button(Button::Rating) => {
                    cx.stats.set_rating(path, (cx.stats.rating(path) + 1) % 6);
                    done.changed = true;
                }
                Focus::Button(Button::Settings) => done.settings = true,
                Focus::SignIn => done.sign_in = true,
                Focus::Badge(_) => {}
            }
        }
        done
    }

    /// Where button `i` is on the touch screen.
    fn button_rect(i: usize) -> (f32, f32, f32, f32) {
        let n = BUTTONS.len() as f32;
        let w = (SW - 2.0 * MARGIN - (n - 1.0) * BUTTON_GAP) / n;
        (MARGIN + i as f32 * (w + BUTTON_GAP), BUTTON_Y, w, BUTTON_H)
    }

    /// Where badge `i` is on the touch screen, if its row is on show.
    fn badge_rect(&self, i: usize) -> Option<(f32, f32, f32, f32)> {
        let row = (i / COLS).checked_sub(self.first).filter(|&r| r < ROWS)?;
        let width = COLS as f32 * PITCH - (PITCH - BADGE);
        let x0 = (SW - width) / 2.0;
        let (col, row) = ((i % COLS) as f32, row as f32);
        Some((x0 + col * PITCH, GRID_Y + row * PITCH, BADGE, BADGE))
    }

    /// The sign-in offered in the grid's place.
    fn sign_in_rect() -> (f32, f32, f32, f32) {
        (SW / 2.0 - 220.0, GRID_Y + 70.0, 440.0, BUTTON_H)
    }

    /// What a tap at `(x, y)` on the touch screen lands on.
    fn hit(&self, x: f32, y: f32, cx: &Context) -> Option<Focus> {
        let inside = |(rx, ry, w, h): (f32, f32, f32, f32)| {
            (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y)
        };
        if let Some(i) = (0..BUTTONS.len()).find(|&i| inside(Self::button_rect(i))) {
            return Some(Focus::Button(BUTTONS[i]));
        }
        if self.offers_sign_in(cx) && inside(Self::sign_in_rect()) {
            return Some(Focus::SignIn);
        }
        (0..cx.badges())
            .find(|&i| self.badge_rect(i).is_some_and(inside))
            .map(Focus::Badge)
    }

    /// The achievement whose badge is chosen, for the back of the box.
    fn chosen(&self) -> Option<usize> {
        match self.focus {
            Focus::Badge(i) => Some(i),
            _ => None,
        }
    }
}

/// The line the grid's place says instead of badges, or none.
fn no_badges(page: &GamePage, cx: &Context) -> Option<String> {
    if !page.achievements {
        let console = cx.game.console.label();
        return Some(format!("No achievements for {console} games here yet"));
    }
    match &cx.cheevos {
        (Some(Answer::Set(set)), _) if set.achievements.is_empty() => {
            Some("No achievements in this game's set".into())
        }
        (Some(Answer::Set(_)), _) => None,
        (_, true) | (None, false) => Some("Looking up achievements\u{2026}".into()),
        (Some(Answer::Unknown), _) => Some("Not on RetroAchievements".into()),
        (Some(Answer::Refused), _) => Some("The sign-in was refused: sign in again".into()),
        (Some(Answer::Nothing), _) if !page.signed_in => {
            Some("Sign in to see this game's achievements".into())
        }
        (Some(Answer::Nothing), _) => Some("No achievements to show".into()),
    }
}

impl Ui<'_> {
    /// Both screens: the back of the box on top, the buttons and badges on the touch screen.
    pub fn draw_game_page(&mut self, page: &GamePage, cx: &Context) -> Result<(), String> {
        let game = cx.game.clone();
        self.cheevos_poll(&game)?;
        let mine = Mine {
            stats: cx.stats,
            set: cx.set(),
            chosen: page.chosen(),
            g: cx.g,
        };
        self.draw_back(&game, &mine)?;

        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let fill = |ui: &mut Self, c: Color, (x, y, w, h): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(255);
            ui.canvases[v.c].copy(&ui.white[v.c], None, v.rect(x, y, w, h))
        };
        let ink = Color::RGB(20, 23, 30);
        for (i, &b) in BUTTONS.iter().enumerate() {
            let on = page.focus == Focus::Button(b);
            let (x, y, w, h) = GamePage::button_rect(i);
            fill(self, if on { WHITE } else { PANEL }, (x, y, w, h))?;
            let text = if on { ink } else { WHITE };
            let mid = x + w / 2.0;
            match b {
                Button::Play => {
                    self.text(v, "Play", (32.0, true), (mid, y + 18.0), 0.5, text)?;
                }
                Button::Favourite => {
                    let fave = cx.stats.fave(&game.path);
                    let heart = if fave { RED } else { Color::RGB(70, 76, 92) };
                    self.heart(v, mid - ICON / 2.0, y + 12.0, heart)?;
                    let label = if fave { "Favourite" } else { "Not a favourite" };
                    self.text(v, label, (18.0, false), (mid, y + 46.0), 0.5, text)?;
                }
                Button::Rating => {
                    let stars = 5.0 * (ICON + 4.0) - 4.0;
                    self.stars(v, mid - stars / 2.0, y + 12.0, cx.stats.rating(&game.path))?;
                    self.text(v, "Rating", (18.0, false), (mid, y + 46.0), 0.5, text)?;
                }
                Button::Settings => {
                    self.text(v, "Settings", (26.0, true), (mid, y + 12.0), 0.5, text)?;
                    let under = if on { ink } else { FAINT };
                    self.text(
                        v,
                        "for this game",
                        (16.0, false),
                        (mid, y + 48.0),
                        0.5,
                        under,
                    )?;
                }
            }
        }

        // The badges: a heading with the standing, then the grid of them.
        self.text(
            v,
            "Achievements",
            (20.0, false),
            (MARGIN, HEADING_Y),
            0.0,
            DIM,
        )?;
        match (cx.set(), no_badges(page, cx)) {
            (Some(set), None) => {
                let n = set.achievements.len();
                let summary = set.summary();
                self.text(
                    v,
                    &summary,
                    (20.0, true),
                    (SW - MARGIN, HEADING_Y),
                    1.0,
                    WHITE,
                )?;
                let black = Color::RGB(0, 0, 0);
                for (i, a) in set.achievements.iter().enumerate() {
                    let Some((x, y, side, _)) = page.badge_rect(i) else {
                        continue;
                    };
                    if page.focus == Focus::Badge(i) {
                        fill(self, WHITE, (x - 7.0, y - 7.0, side + 14.0, side + 14.0))?;
                        fill(self, black, (x - 3.0, y - 3.0, side + 6.0, side + 6.0))?;
                    } else if a.unlocked {
                        fill(self, GOLD, (x - 3.0, y - 3.0, side + 6.0, side + 6.0))?;
                        fill(self, black, (x - 1.0, y - 1.0, side + 2.0, side + 2.0))?;
                    }
                    self.badge(v, &a.badge, a.unlocked, (x, y, side))?;
                }
                // More rows than show: which these are.
                let rows = n.div_ceil(COLS);
                if rows > ROWS {
                    let shown = format!(
                        "Rows {}\u{2013}{} of {rows}",
                        page.first + 1,
                        page.first + ROWS
                    );
                    let y = GRID_Y + ROWS as f32 * PITCH - 4.0;
                    self.text(v, &shown, (16.0, false), (SW - MARGIN, y), 1.0, FAINT)?;
                }
            }
            (_, Some(why)) => {
                self.text(v, &why, (24.0, false), (SW / 2.0, GRID_Y + 10.0), 0.5, DIM)?;
                if page.offers_sign_in(cx) {
                    let on = page.focus == Focus::SignIn;
                    let (x, y, w, h) = GamePage::sign_in_rect();
                    fill(self, if on { WHITE } else { PANEL }, (x, y, w, h))?;
                    let label = match cx.cheevos.0 {
                        Some(Answer::Refused) => "Sign in again",
                        _ => "Sign in to RetroAchievements",
                    };
                    let text = if on { ink } else { WHITE };
                    self.text(v, label, (24.0, true), (x + w / 2.0, y + 24.0), 0.5, text)?;
                }
            }
            (None, None) => {}
        }
        let hint = "D-pad  move  \u{b7}  L / R  buttons  \u{b7}  A  choose  \u{b7}  Y  look up again  \u{b7}  Back  settings  \u{b7}  B  close";
        self.footer(v, hint, 0.0, game.console)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::set::Achievement;
    use crate::rom::Console;
    use crate::testing::TempDir;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn game(dir: &TempDir) -> Game {
        Game {
            console: Console::Gba,
            path: dir.path().join("Golden Sun.gba"),
            title: vec!["Golden Sun".into()],
            stem: "Golden Sun".into(),
            code: String::new(),
            icon: None,
            cover: None,
            gone: false,
        }
    }

    /// A set of `n` achievements, the first earned.
    fn set(n: u32) -> Answer {
        let one = |id: u32| Achievement {
            id,
            title: format!("Achievement {id}"),
            description: String::new(),
            points: 5,
            badge: String::new(),
            unlocked: id == 1,
            unlocked_at: None,
        };
        Answer::Set(Arc::new(Set {
            id: 1,
            title: "Golden Sun".into(),
            achievements: (1..=n).map(one).collect(),
        }))
    }

    fn input(f: impl FnOnce(&mut Input)) -> Input {
        let mut i = Input::default();
        f(&mut i);
        i
    }

    /// The buttons: left and right along them, A presses; Favourite and Rating change the
    /// game's own, Play and Settings are the shelf's to do.
    #[test]
    fn the_buttons() {
        let dir = TempDir::new("game-page");
        let game = game(&dir);
        let mut stats = Stats::load(dir.path().join("stats.tsv"));
        let mut cx = Context {
            stats: &mut stats,
            game: &game,
            g: 0,
            cheevos: (Some(set(4)), false),
        };
        let mut page = GamePage::open(&game, true);
        let a = input(|i| i.pressed = true);
        let right = input(|i| i.turn = 1);
        assert!(page.update(&a, &mut cx).play);
        page.update(&right, &mut cx);
        assert!(page.update(&a, &mut cx).changed);
        assert!(cx.stats.fave(&game.path));
        page.update(&right, &mut cx);
        for _ in 0..6 {
            page.update(&a, &mut cx);
        }
        assert_eq!(cx.stats.rating(&game.path), 0);
        page.update(&a, &mut cx);
        assert_eq!(cx.stats.rating(&game.path), 1);
        page.update(&right, &mut cx);
        page.update(&right, &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Settings));
        assert!(page.update(&a, &mut cx).settings);
        // L / R: along the buttons, round the row.
        let (l, r) = (input(|i| i.page = -1), input(|i| i.page = 1));
        page.update(&r, &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Play));
        page.update(&l, &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Settings));
        page.update(&l, &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Rating));
        assert!(page.update(&input(|i| i.reset = true), &mut cx).look_again);
        assert!(page.update(&input(|i| i.close = true), &mut cx).closed);
        // A tap on a button presses it.
        let (x, y, w, h) = GamePage::button_rect(0);
        let tap = input(|i| i.taps = vec![(x + w / 2.0, y + h / 2.0)]);
        assert!(page.update(&tap, &mut cx).play);
    }

    /// The badges: down from the buttons into the grid, along and down its rows (the rows on
    /// show following), up off its top to the button above.
    #[test]
    fn the_badges() {
        let dir = TempDir::new("game-page-badges");
        let game = game(&dir);
        let mut stats = Stats::load(dir.path().join("stats.tsv"));
        let n = (COLS * (ROWS + 2) + 3) as u32;
        let mut cx = Context {
            stats: &mut stats,
            game: &game,
            g: 0,
            cheevos: (Some(set(n)), false),
        };
        let mut page = GamePage::open(&game, true);
        let down = input(|i| i.moved = 1);
        page.update(&down, &mut cx);
        assert_eq!(page.focus, Focus::Badge(0));
        page.update(&input(|i| i.turn = 1), &mut cx);
        page.update(&down, &mut cx);
        assert_eq!(page.focus, Focus::Badge(COLS + 1));
        for _ in 0..ROWS + 4 {
            page.update(&down, &mut cx);
        }
        // Down the second column to the last row, which has three.
        assert_eq!(page.focus, Focus::Badge(COLS * (ROWS + 2) + 1));
        assert_eq!(page.first, (n as usize - 1) / COLS + 1 - ROWS);
        assert!(page.badge_rect(n as usize - 1).is_some());
        assert!(page.badge_rect(0).is_none());
        for _ in 0..ROWS + 4 {
            page.update(&input(|i| i.moved = -1), &mut cx);
        }
        assert!(matches!(page.focus, Focus::Button(_)));
        assert_eq!(page.first, 0);
        // From a badge, L / R go to the buttons: the one after the button above it.
        page.update(&down, &mut cx);
        assert_eq!(page.focus, Focus::Badge(0));
        page.update(&input(|i| i.page = 1), &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Favourite));
        // A tap on a badge picks it.
        let (x, y, _, _) = page.badge_rect(2).unwrap();
        page.update(&input(|i| i.taps = vec![(x + 1.0, y + 1.0)]), &mut cx);
        assert_eq!(page.focus, Focus::Badge(2));
        assert_eq!(page.chosen(), Some(2));
        // The set gone (signed out): back up to the buttons.
        cx.cheevos = (None, true);
        page.update(&Input::default(), &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Play));
    }

    /// No set: why, and a sign-in where one is wanted.
    #[test]
    fn no_set() {
        let dir = TempDir::new("game-page-none");
        let game = game(&dir);
        let mut stats = Stats::load(dir.path().join("stats.tsv"));
        let mut cx = Context {
            stats: &mut stats,
            game: &game,
            g: 0,
            cheevos: (Some(Answer::Nothing), false),
        };
        let mut page = GamePage::open(&game, false);
        if !page.achievements {
            let why = no_badges(&page, &cx).unwrap_or_default();
            assert!(why.starts_with("No achievements for GBA"));
            return;
        }
        assert_eq!(
            no_badges(&page, &cx).as_deref(),
            Some("Sign in to see this game's achievements")
        );
        page.update(&input(|i| i.moved = 1), &mut cx);
        assert_eq!(page.focus, Focus::SignIn);
        assert!(page.update(&input(|i| i.pressed = true), &mut cx).sign_in);
        cx.cheevos = (Some(Answer::Unknown), false);
        let why = no_badges(&page, &cx);
        assert_eq!(why.as_deref(), Some("Not on RetroAchievements"));
        page.update(&Input::default(), &mut cx);
        assert_eq!(page.focus, Focus::Button(Button::Play));
        // A system the service doesn't hash: no achievements at all.
        let mut gw = game.clone();
        gw.console = Console::GameWatch;
        gw.path = PathBuf::from("/r/GW/Fire.mgw");
        assert!(!GamePage::open(&gw, true).achievements);
    }
}
