//! The selected game's achievements on the top screen (achievements/): under its details, a
//! line counting them and a bar of how far through the set the player is; and the badges, for
//! the game page (game.rs, back.rs) to draw. The lookups are the worker's (achievements/lookup.rs); what is here
//! is their badges as textures and the drawing.

use super::draw::{destroy, texture, Ui, DIM, FAINT, GOLD, WHITE};
use super::layout::*;
use crate::achievements::lookup::Lookups;
use crate::achievements::set::{Answer, Set};
use crate::achievements::Ra;
use crate::rom::Game;
use sdl2::render::Texture;
use std::collections::HashMap;
use std::path::Path;

/// The games' achievements while the shelf is up: the lookups, and the badges in so far as
/// textures, by name, one for each canvas (the game page draws them on both screens).
pub struct Cheevos {
    lookups: Lookups,
    badges: HashMap<String, Vec<Texture>>,
}

impl Cheevos {
    /// Starts the lookups' worker, on what `ra` keeps.
    pub fn start(ra: &Ra) -> Cheevos {
        Cheevos {
            lookups: Lookups::start(ra.clone()),
            badges: HashMap::new(),
        }
    }

    /// Whether a lookup or a badge is still to come in.
    pub fn waiting(&self) -> bool {
        self.lookups.waiting()
    }
}

impl Ui<'_> {
    /// What the lookup for the game at `path` came to, and whether it is still being looked
    /// up: for the game page's Achievements tab.
    pub fn cheevos_answer(&self, path: &Path) -> (Option<Answer>, bool) {
        let lookups = &self.cheevos.lookups;
        (lookups.answer(path).cloned(), lookups.looking(path))
    }

    /// Forgets every game's achievements, to be looked up again: the player signed in or out.
    pub fn forget_cheevos(&mut self) {
        self.cheevos.lookups.forget();
    }

    /// Takes in what the worker found, and asks about `game` unless it has been already.
    pub(super) fn cheevos_poll(&mut self, game: &Game) -> Result<(), String> {
        for (name, img) in self.cheevos.lookups.poll() {
            let made: Result<Vec<_>, _> = self.creators.iter().map(|c| texture(c, &img)).collect();
            if let Some(old) = self.cheevos.badges.insert(name, made?) {
                old.into_iter().for_each(destroy);
            }
        }
        self.cheevos.lookups.ask(game.console, &game.path);
        Ok(())
    }

    /// One badge, `side` units square at (x, y): dimmed while locked.
    pub(super) fn badge(
        &mut self,
        v: View,
        name: &str,
        unlocked: bool,
        (x, y, side): (f32, f32, f32),
    ) -> Result<(), String> {
        let Some(t) = self.cheevos.badges.get_mut(name).map(|ts| &mut ts[v.c]) else {
            // Not in yet: its place, faintly.
            self.white[v.c].set_color_mod(255, 255, 255);
            self.white[v.c].set_alpha_mod(if unlocked { 40 } else { 18 });
            return self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, side, side));
        };
        if unlocked {
            t.set_color_mod(255, 255, 255);
            t.set_alpha_mod(255);
        } else {
            t.set_color_mod(110, 110, 110);
            t.set_alpha_mod(140);
        }
        self.canvases[v.c].copy(t, None, v.rect(x, y, side, side))
    }

    /// "Achievements" and a line under it, faint: what there is instead of a set.
    fn cheevos_line(&mut self, line: &str, (x, y): (f32, f32)) -> Result<(), String> {
        let v = self.top;
        self.text(v, "Achievements", (22.0, false), (x, y), 0.0, DIM)?;
        self.text(v, line, (22.0, false), (x, y + 30.0), 0.0, FAINT)
    }

    /// The game's achievements on the top screen, under its details at `(x, y)`: a line
    /// counting them and a bar of how far through the set the player is.
    pub(super) fn draw_cheevos(&mut self, game: &Game, (x, y): (f32, f32)) -> Result<(), String> {
        self.cheevos_poll(game)?;
        let answer = self.cheevos.lookups.answer(&game.path).cloned();
        let set = match answer {
            Some(Answer::Set(set)) => set,
            None if self.cheevos.lookups.looking(&game.path) => {
                return self.cheevos_line("Looking up\u{2026}", (x, y));
            }
            Some(Answer::Unknown) => return self.cheevos_line("Not on RetroAchievements", (x, y)),
            Some(Answer::Refused) => {
                return self.cheevos_line("Sign in again in the menu", (x, y));
            }
            Some(Answer::Nothing) | None => return Ok(()),
        };
        self.draw_cheevos_bar(&set, (x, y))
    }

    /// The count and the bar.
    fn draw_cheevos_bar(&mut self, set: &Set, (x, y): (f32, f32)) -> Result<(), String> {
        let v = self.top;
        let width = SW - 40.0 - x;
        let n = set.achievements.len();
        self.text(v, "Achievements", (22.0, false), (x, y), 0.0, DIM)?;
        self.text(v, &set.summary(), (22.0, true), (x, y + 28.0), 0.0, WHITE)?;
        // How far through the set, by count: a faint track and the part earned in gold.
        let (bar_y, bar_h) = (y + 66.0, 6.0);
        let done = if n == 0 {
            0.0
        } else {
            set.unlocked() as f32 / n as f32
        };
        let white = &mut self.white[v.c];
        white.set_color_mod(255, 255, 255);
        white.set_alpha_mod(28);
        self.canvases[v.c].copy(white, None, v.rect(x, bar_y, width, bar_h))?;
        if done > 0.0 {
            white.set_color_mod(GOLD.r, GOLD.g, GOLD.b);
            white.set_alpha_mod(255);
            self.canvases[v.c].copy(white, None, v.rect(x, bar_y, width * done, bar_h))?;
            white.set_color_mod(255, 255, 255);
        }
        Ok(())
    }
}
