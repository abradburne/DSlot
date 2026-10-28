//! The back of the game's box, on the top screen while its game page is open (game.rs): X
//! turns the box on the shelf round. As a real one's: the title along a band in the system's
//! colour with the player's rating and favourite heart, two screenshots in white borders set at
//! an angle (its title screen and one in play), the blurb (Wikipedia's lead), the player's own
//! play as the points down the bottom, and a barcode. The achievement chosen on the touch
//! screen goes on it as a gold sticker.
//!
//! What it says comes from the web (details.rs), looked up on a thread of its own when the page
//! opens; what was kept from before shows at once.

use super::draw::{console_colour, destroy, dim_by, texture, Ui, DIM, FAINT, GOLD, RED, WHITE};
use super::layout::*;
use crate::achievements::set::Set;
use crate::details::{self, Details, Looked};
use crate::gfx;
use crate::rom::Game;
use crate::stats::Stats;
use sdl2::pixels::Color;
use sdl2::render::Texture;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// How long the note of what a look-up asked for again found stays on the box.
const NOTE_FOR: Duration = Duration::from_secs(6);

/// The box back's details, and their screenshots as textures on the top screen's canvas.
pub struct Back {
    cache: PathBuf,
    /// The game whose details are being looked up, the answer to come, and whether they are
    /// asked for again (Y).
    asked: Option<(PathBuf, Receiver<Looked>, bool)>,
    /// A note on the box: that the details are being looked up again, then what that found,
    /// and since when it shows.
    note: Option<(String, Option<Instant>)>,
    /// The game shown, its index in the shelf's games, and what is known of it.
    shown: Option<(PathBuf, usize, Details)>,
    /// Its screenshots (title screen, then in play), each with its size in pixels.
    shots: Vec<(Texture, f32, f32)>,
}

impl Back {
    pub fn new(cache: PathBuf) -> Back {
        Back {
            cache,
            asked: None,
            note: None,
            shown: None,
            shots: Vec::new(),
        }
    }

    /// Whether details are still to come in, or a note is still to go: the shelf keeps drawing
    /// for them.
    pub fn waiting(&self) -> bool {
        self.asked.is_some() || self.note.is_some()
    }

    /// The note to show now, if any; one shown long enough is let go.
    fn note(&mut self) -> Option<String> {
        if let Some((_, Some(at))) = &self.note {
            if at.elapsed() >= NOTE_FOR {
                self.note = None;
            }
        }
        self.note.as_ref().map(|(text, _)| text.clone())
    }
}

/// What the box back shows that is the player's own, and the chosen achievement.
pub struct Mine<'a> {
    pub stats: &'a Stats,
    pub set: Option<&'a Set>,
    /// The achievement chosen on the touch screen, by its place in the set.
    pub chosen: Option<usize>,
    /// Its index in the shelf's games, for its box when there are no screenshots.
    pub g: usize,
}

impl Ui<'_> {
    /// The game page opened on `game`: what is kept of its details at once, and the rest
    /// looked up.
    pub fn open_back(&mut self, game: &Game, g: usize) {
        self.close_back();
        let cache = self.back.cache.clone();
        let kept = details::kept(&cache, game);
        self.back_shots(&kept);
        self.back.shown = Some((game.path.clone(), g, kept));
        self.ask_back(game, false);
    }

    /// Y on the game page: `game`'s details asked for afresh, with a note on the box while they
    /// are and of what was found after. Not while a look-up is already under way.
    pub fn look_up_again(&mut self, game: &Game) {
        if self.back.asked.is_none() {
            self.ask_back(game, true);
            self.back.note = Some(("Looking this game up again\u{2026}".into(), None));
        }
    }

    /// `game`'s details looked up on a thread of their own, `again` or not.
    fn ask_back(&mut self, game: &Game, again: bool) {
        let cache = self.back.cache.clone();
        let (tell, told) = channel();
        let game = game.clone();
        let path = game.path.clone();
        std::thread::spawn(move || {
            let _ = tell.send(details::look_up(&cache, &game, again));
        });
        self.back.asked = Some((path, told, again));
    }

    /// The game page closed: its screenshots let go of.
    pub fn close_back(&mut self) {
        self.back.shots.drain(..).for_each(|(t, _, _)| destroy(t));
        (self.back.shown, self.back.asked, self.back.note) = (None, None, None);
    }

    /// The screenshots of `details` as textures.
    fn back_shots(&mut self, details: &Details) {
        self.back.shots.drain(..).for_each(|(t, _, _)| destroy(t));
        let files = [&details.title_shot, &details.snap];
        for file in files.into_iter().flatten() {
            let Some(img) = gfx::load_png(file) else {
                continue;
            };
            if let Ok(t) = texture(&self.creators[self.top.c], &img) {
                self.back.shots.push((t, img.w as f32, img.h as f32));
            }
        }
    }

    /// Takes in the details looked up, if they have come.
    fn back_poll(&mut self) {
        let Some((path, told, again)) = &self.back.asked else {
            return;
        };
        let again = *again;
        let looked = match told.try_recv() {
            Ok(looked) => looked,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.back.asked = None;
                return;
            }
        };
        let path = path.clone();
        self.back.asked = None;
        let Some((_, g, was)) = self.back.shown.take().filter(|(p, _, _)| *p == path) else {
            return;
        };
        if again {
            let line = details::found_line(&looked);
            self.back.note = Some((line, Some(Instant::now())));
        }
        // Box art fetched for a game with none (or fetched again, or gone): its cart and box
        // are made again with it.
        let details = looked.details;
        if was.boxart != details.boxart || again && details.boxart.is_some() {
            self.forget_art_of(g);
        }
        self.back_shots(&details);
        self.back.shown = Some((path, g, details));
    }

    /// The back of `game`'s box, on the top screen.
    pub fn draw_back(&mut self, game: &Game, mine: &Mine) -> Result<(), String> {
        self.back_poll();
        let v = self.top;
        let colour = console_colour(game.console);
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let fill = |ui: &mut Self, c: Color, (x, y, w, h): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(255);
            ui.canvases[v.c].copy(&ui.white[v.c], None, v.rect(x, y, w, h))
        };
        let details = self
            .back
            .shown
            .as_ref()
            .filter(|(p, _, _)| *p == game.path)
            .map(|(_, _, d)| d.clone())
            .unwrap_or_default();
        let looking = self.back.asked.is_some();

        // The box: its back in the system's colour, darker, the spine down the right, the band
        // along the top.
        fill(self, Color::RGB(0, 0, 0), (0.0, 0.0, SW, SH))?;
        let (bx, by, bw, bh) = (40.0, 24.0, SW - 80.0, SH - 48.0);
        fill(self, dim_by(colour, 0.42), (bx, by, bw, bh))?;
        fill(self, Color::RGB(16, 18, 24), (bx + bw - 30.0, by, 30.0, bh))?;
        fill(self, colour, (bx, by, bw - 30.0, 100.0))?;
        let path = &game.path;
        let stars = 5.0 * (ICON + 4.0) - 4.0;
        let fave = mine.stats.fave(path);
        let right = bx + bw - 30.0 - 24.0;
        self.stars(v, right - stars, by + 18.0, mine.stats.rating(path))?;
        if fave {
            self.heart(v, right - stars - ICON - 14.0, by + 18.0, RED)?;
        }
        // The one played longest of all gets a ribbon.
        let secs = mine.stats.secs(path);
        if secs > 0 && secs >= mine.stats.most().0 {
            let ribbon = "Your most played game";
            self.text(v, ribbon, (18.0, true), (right, by + 64.0), 1.0, GOLD)?;
        }
        let room = right - stars - 60.0 - (bx + 36.0);
        let size = self.fonts.fit(game.name(), 46.0, 24.0, room, true);
        let at = (bx + 36.0, by + 14.0 + (46.0 - size) / 2.0);
        self.text(v, game.name(), (size, true), at, 0.0, WHITE)?;
        let about = details.about.as_ref();
        let facts: Vec<String> = [
            about.and_then(|a| a.year.clone()),
            Some(game.console.name().to_owned()),
            about.and_then(|a| a.developer.clone()),
            about
                .and_then(|a| a.publisher.clone())
                .filter(|p| about.and_then(|a| a.developer.as_ref()) != Some(p)),
            about.and_then(|a| a.genre.clone()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let facts = facts.join("  \u{b7}  ");
        let size = self.fonts.fit(&facts, 20.0, 15.0, bw - 30.0 - 72.0, false);
        self.text(
            v,
            &facts,
            (size, false),
            (bx + 38.0, by + 66.0),
            0.0,
            light(colour, 0.75),
        )?;

        // The screenshots, in white borders, at an angle; without any, the front of the box.
        let shots: Vec<(usize, f32, f32)> = self
            .back
            .shots
            .iter()
            .enumerate()
            .map(|(i, (_, w, h))| (i, *w, *h))
            .collect();
        // Each shot's centre, the most room it may take, and its angle.
        type Place = ((f32, f32), (f32, f32), f64);
        let places: &[Place] = match shots.len() {
            0 => &[],
            1 => &[((470.0, 300.0), (540.0, 310.0), -2.0)],
            _ => &[
                ((300.0, 290.0), (400.0, 270.0), -4.0),
                ((660.0, 318.0), (380.0, 256.0), 3.0),
            ],
        };
        for (&(i, w, h), &((cx, cy), (mw, mh), angle)) in shots.iter().zip(places) {
            let k = (mw / w).min(mh / h);
            let (sw, sh) = (w * k, h * k);
            let border = 10.0;
            let frame = v.rect(
                cx - sw / 2.0 - border,
                cy - sh / 2.0 - border,
                sw + 2.0 * border,
                sh + 2.0 * border,
            );
            let shadow = {
                let mut r = frame;
                r.offset((8.0 * v.s) as i32, (10.0 * v.s) as i32);
                r
            };
            self.white[v.c].set_color_mod(0, 0, 0);
            self.white[v.c].set_alpha_mod(110);
            self.canvases[v.c].copy_ex(
                &self.white[v.c],
                None,
                shadow,
                angle,
                None,
                false,
                false,
            )?;
            self.white[v.c].set_color_mod(244, 244, 238);
            self.white[v.c].set_alpha_mod(255);
            self.canvases[v.c].copy_ex(&self.white[v.c], None, frame, angle, None, false, false)?;
            let dst = v.rect(cx - sw / 2.0, cy - sh / 2.0, sw, sh);
            let (shot, _, _) = &self.back.shots[i];
            self.canvases[v.c].copy_ex(shot, None, dst, angle, None, false, false)?;
        }
        if shots.is_empty() {
            // Centred, or to the left of a sticker.
            let mid = if mine.chosen.is_some() {
                280.0
            } else {
                SW / 2.0 - 15.0
            };
            self.draw_box_in(mine.g, game.console, (mid - 130.0, 148.0, 260.0, 280.0))?;
            let why = if looking {
                "Looking for screenshots\u{2026}"
            } else {
                "No screenshots found for this game"
            };
            self.text(v, why, (18.0, false), (mid, 440.0), 0.5, DIM)?;
        }

        // The blurb, and where it is from.
        let (tx, width) = (bx + 36.0, bw - 30.0 - 72.0);
        let mut y = 486.0;
        match about.filter(|a| !a.summary.is_empty()) {
            Some(about) => {
                for line in self.fonts.wrap(&about.summary, 20.0, false, width, 4) {
                    let ink = Color::RGB(226, 229, 236);
                    self.text(v, &line, (20.0, false), (tx, y), 0.0, ink)?;
                    y += 27.0;
                }
                let credit = format!("From Wikipedia, \u{201c}{}\u{201d}  (CC BY-SA)", about.page);
                self.text(
                    v,
                    &credit,
                    (13.0, false),
                    (tx, y + 2.0),
                    0.0,
                    light(colour, 0.35),
                )?;
            }
            None => {
                let why = if looking {
                    "Looking this game up\u{2026}"
                } else {
                    "Nothing found about this game online yet: with Wi-Fi on, it is looked up when this page opens."
                };
                for line in self.fonts.wrap(why, 20.0, false, width, 2) {
                    self.text(v, &line, (20.0, false), (tx, y), 0.0, DIM)?;
                    y += 27.0;
                }
            }
        }

        // A look-up asked for again: under way, then what it found, on a label across the box.
        if let Some(note) = self.back.note() {
            let w = self.fonts.width(&note, 20.0, true) + 48.0;
            let (nx, ny) = (bx + (bw - 30.0 - w) / 2.0, by + 112.0);
            fill(self, GOLD, (nx, ny, w, 40.0))?;
            fill(
                self,
                Color::RGB(24, 26, 34),
                (nx + 3.0, ny + 3.0, w - 6.0, 34.0),
            )?;
            self.text(v, &note, (20.0, true), (nx + 24.0, ny + 8.0), 0.0, GOLD)?;
        }

        // The points down the bottom: the player's own play.
        let sessions = match mine.stats.session_count(path) {
            1 => "1 session".to_owned(),
            n => format!("{n} sessions"),
        };
        let played = match mine.stats.secs(path) {
            0 => "Not played yet".to_owned(),
            _ => format!("{} played", mine.stats.played(path)),
        };
        let mut points = vec![
            played,
            sessions,
            format!("Last played: {}", mine.stats.last_played(path)),
        ];
        if let Some(set) = mine.set {
            let n = set.achievements.len();
            points.push(format!("{} of {n} achievements", set.unlocked()));
        }
        for (i, point) in points.iter().enumerate() {
            let (px, py) = (tx + (i / 2) as f32 * 330.0, 642.0 + (i % 2) as f32 * 38.0);
            fill(self, GOLD, (px, py + 7.0, 10.0, 10.0))?;
            self.text(v, point, (20.0, true), (px + 22.0, py), 0.0, WHITE)?;
        }
        self.barcode(game, (bx + bw - 30.0 - 200.0, 636.0, 170.0, 70.0))?;

        // The achievement chosen, as a sticker on the box.
        if let (Some(i), Some(set)) = (mine.chosen, mine.set) {
            if let Some(a) = set.achievements.get(i).cloned() {
                let (sx, sy, sw, sh) = (548.0, 268.0, 400.0, 196.0);
                fill(self, GOLD, (sx, sy, sw, sh))?;
                fill(
                    self,
                    Color::RGB(24, 26, 34),
                    (sx + 4.0, sy + 4.0, sw - 8.0, sh - 8.0),
                )?;
                self.badge(v, &a.badge, a.unlocked, (sx + 18.0, sy + 18.0, 80.0))?;
                let ax = sx + 116.0;
                let size = self.fonts.fit(&a.title, 26.0, 16.0, sw - 132.0, true);
                self.text(v, &a.title, (size, true), (ax, sy + 18.0), 0.0, WHITE)?;
                let points = match a.points {
                    1 => "1 point".to_owned(),
                    n => format!("{n} points"),
                };
                let (state, c) = match a.unlocked {
                    true => (format!("{points}  \u{b7}  Earned!"), GOLD),
                    false => (format!("{points}  \u{b7}  Not earned yet"), FAINT),
                };
                self.text(v, &state, (18.0, true), (ax, sy + 56.0), 0.0, c)?;
                let mut dy = sy + 110.0;
                for line in self.fonts.wrap(&a.description, 18.0, false, sw - 36.0, 3) {
                    self.text(v, &line, (18.0, false), (sx + 18.0, dy), 0.0, DIM)?;
                    dy += 24.0;
                }
            }
        }
        Ok(())
    }

    /// A barcode on a white label, its bars from the game's code (or name), the code under it.
    fn barcode(&mut self, game: &Game, (x, y, w, h): (f32, f32, f32, f32)) -> Result<(), String> {
        let v = self.top;
        self.white[v.c].set_color_mod(244, 244, 238);
        self.white[v.c].set_alpha_mod(255);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, w, h))?;
        let code = match game.serial() {
            s if s.is_empty() => game.stem.clone(),
            s => s,
        };
        // Bars of one to three units' width, spaces of one or two, from the code's bytes.
        let mut seed = code
            .bytes()
            .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let (left, bars_h) = (x + 10.0, h - 26.0);
        let unit = 1.6;
        let mut bx = left;
        self.white[v.c].set_color_mod(10, 10, 12);
        while bx < x + w - 14.0 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let bar = (seed % 3 + 1) as f32 * unit;
            let gap = ((seed >> 8) % 2 + 1) as f32 * unit;
            if bx + bar > x + w - 10.0 {
                break;
            }
            self.canvases[v.c].copy(&self.white[v.c], None, v.rect(bx, y + 6.0, bar, bars_h))?;
            bx += bar + gap;
        }
        let ink = Color::RGB(10, 10, 12);
        let size = self.fonts.fit(&code, 14.0, 9.0, w - 16.0, false);
        self.text(
            v,
            &code,
            (size, false),
            (x + w / 2.0, y + h - 19.0),
            0.5,
            ink,
        )
    }
}

/// A colour `k` of the way to white.
pub(super) fn light(c: Color, k: f32) -> Color {
    let up = |v: u8| (v as f32 + (255.0 - v as f32) * k) as u8;
    Color::RGB(up(c.r), up(c.g), up(c.b))
}
