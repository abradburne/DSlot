//! The selected game's achievements on the top screen (achievements/): under its details, a
//! line counting them and a bar of how far through the set the player is; or, while the menu's
//! Game page browses them, the list in the details' place, each with its badge, title, points
//! and description. The lookups are the worker's (achievements/lookup.rs); what is here is
//! their badges as textures and the drawing.
//!
//! The list's place ([`ListView`]: the row highlighted and the first shown) is the menu's,
//! moved by its buttons; this only draws it.

use super::draw::{destroy, texture, Ui, DIM, FAINT, GOLD, WHITE};
use super::layout::*;
use crate::achievements::lookup::Lookups;
use crate::achievements::set::{Answer, Set};
use crate::achievements::Ra;
use crate::rom::Game;
use sdl2::render::Texture;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Rows the browsed list shows at once.
pub const LIST_ROWS: usize = 6;

/// Where the achievements list is while it is browsed: the achievement highlighted, and the
/// first of the [`LIST_ROWS`] shown, which follows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListView {
    pub at: usize,
    pub first: usize,
}

impl ListView {
    /// Moves the highlight `by` rows along a list of `n`, stopping at either end; the rows
    /// shown follow it.
    pub fn move_by(&mut self, by: i32, n: usize) {
        let last = n.saturating_sub(1) as i64;
        self.at = (self.at as i64 + by as i64).clamp(0, last) as usize;
        self.follow(n);
    }

    /// The first row shown, moved as little as keeps the highlight in sight.
    fn follow(&mut self, n: usize) {
        if self.at < self.first {
            self.first = self.at;
        } else if self.at >= self.first + LIST_ROWS {
            self.first = self.at + 1 - LIST_ROWS;
        }
        self.first = self.first.min(n.saturating_sub(LIST_ROWS));
    }

    /// Itself within a list of `n` (a set looked up again may have fewer).
    fn within(mut self, n: usize) -> ListView {
        self.at = self.at.min(n.saturating_sub(1));
        self.follow(n);
        self
    }
}

/// The games' achievements while the shelf is up: the lookups, and the badges in so far as
/// textures, by name, on the top screen's canvas.
pub struct Cheevos {
    lookups: Lookups,
    badges: HashMap<String, Texture>,
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
    /// The game's set, once the worker has it: for the menu's Game page, its row and its list.
    pub fn cheevos_set(&self, path: &Path) -> Option<Arc<Set>> {
        match self.cheevos.lookups.answer(path)? {
            Answer::Set(set) => Some(Arc::clone(set)),
            _ => None,
        }
    }

    /// Forgets every game's achievements, to be looked up again: the player signed in or out.
    pub fn forget_cheevos(&mut self) {
        self.cheevos.lookups.forget();
    }

    /// Takes in what the worker found, and asks about `game` unless it has been already.
    fn cheevos_poll(&mut self, game: &Game) -> Result<(), String> {
        for (name, img) in self.cheevos.lookups.poll() {
            let t = texture(&self.creators[self.top.c], &img)?;
            if let Some(old) = self.cheevos.badges.insert(name, t) {
                destroy(old);
            }
        }
        self.cheevos.lookups.ask(game.console, &game.path);
        Ok(())
    }

    /// One badge, `side` units square at (x, y): dimmed while locked.
    fn badge(
        &mut self,
        v: View,
        name: &str,
        unlocked: bool,
        (x, y, side): (f32, f32, f32),
    ) -> Result<(), String> {
        let Some(t) = self.cheevos.badges.get_mut(name) else {
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
    /// counting them and a bar of how far through the set the player is; or, while the menu
    /// browses them (`list`), the list itself, from `y_list` down.
    pub(super) fn draw_cheevos(
        &mut self,
        game: &Game,
        list: Option<ListView>,
        (x, y): (f32, f32),
        y_list: f32,
    ) -> Result<(), String> {
        self.cheevos_poll(game)?;
        let answer = self.cheevos.lookups.answer(&game.path).cloned();
        let set = match answer {
            Some(Answer::Set(set)) => set,
            // Browsing a list that is no longer there: nothing in the details' place.
            _ if list.is_some() => return Ok(()),
            None if self.cheevos.lookups.looking(&game.path) => {
                return self.cheevos_line("Looking up\u{2026}", (x, y));
            }
            Some(Answer::Unknown) => return self.cheevos_line("Not on RetroAchievements", (x, y)),
            Some(Answer::Refused) => {
                return self.cheevos_line("Sign in again in the menu", (x, y));
            }
            Some(Answer::Nothing) | None => return Ok(()),
        };
        match list {
            Some(list) => self.draw_cheevos_list(&set, list, (x, y_list)),
            None => self.draw_cheevos_bar(&set, (x, y)),
        }
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

    /// The list, from `y` down: the standing, then each achievement shown with its badge,
    /// title, points and description, the highlighted one lit.
    fn draw_cheevos_list(
        &mut self,
        set: &Set,
        list: ListView,
        (x, y): (f32, f32),
    ) -> Result<(), String> {
        let v = self.top;
        let width = SW - 40.0 - x;
        let n = set.achievements.len();
        let ListView { at, first } = list.within(n);
        // The menu's title says these are the achievements: the standing alone here.
        self.text(v, &set.summary(), (22.0, true), (x, y), 0.0, WHITE)?;
        let (row_h, side) = (54.0, 44.0);
        let mut ry = y + 40.0;
        let shown = set.achievements.iter().enumerate().skip(first);
        for (i, a) in shown.take(LIST_ROWS) {
            if i == at {
                self.white[v.c].set_color_mod(255, 255, 255);
                self.white[v.c].set_alpha_mod(28);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(x - 8.0, ry - 4.0, width + 16.0, row_h - 2.0),
                )?;
            }
            self.badge(v, &a.badge, a.unlocked, (x, ry, side))?;
            let tx = x + side + 14.0;
            let points = a.points.to_string();
            let pw = self.fonts.width(&points, 20.0, true) + 12.0;
            let colour = if a.unlocked { WHITE } else { DIM };
            let size = self
                .fonts
                .fit(&a.title, 22.0, 18.0, width - side - 14.0 - pw, true);
            self.text(v, &a.title, (size, true), (tx, ry - 2.0), 0.0, colour)?;
            let gold = if a.unlocked { GOLD } else { FAINT };
            self.text(v, &points, (20.0, true), (x + width, ry), 1.0, gold)?;
            let desc = self
                .fonts
                .wrap(&a.description, 17.0, false, width - side - 14.0, 1);
            if let Some(line) = desc.first() {
                self.text(v, line, (17.0, false), (tx, ry + 26.0), 0.0, FAINT)?;
            }
            ry += row_h;
        }
        if n > LIST_ROWS {
            let where_ = format!("{}\u{2013}{} of {n}", first + 1, (first + LIST_ROWS).min(n));
            self.text(v, &where_, (18.0, false), (x + width, y + 4.0), 1.0, FAINT)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_moves_within_the_set_and_the_rows_shown_follow() {
        let mut list = ListView::default();
        list.move_by(-3, 10);
        assert_eq!(list, ListView { at: 0, first: 0 });
        list.move_by(5, 10);
        assert_eq!(list, ListView { at: 5, first: 0 });
        list.move_by(1, 10);
        assert_eq!(list, ListView { at: 6, first: 1 });
        list.move_by(20, 10);
        assert_eq!(list, ListView { at: 9, first: 4 });
        list.move_by(-6, 10);
        assert_eq!(list, ListView { at: 3, first: 3 });
        // A short list shows from its top.
        let mut short = ListView::default();
        short.move_by(2, 3);
        assert_eq!(short, ListView { at: 2, first: 0 });
        // The set looked up again with fewer: drawn within it.
        let far = ListView { at: 9, first: 4 };
        assert_eq!(far.within(4), ListView { at: 3, first: 0 });
        assert_eq!(ListView::default().within(0), ListView::default());
    }
}
