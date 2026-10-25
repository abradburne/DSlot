//! Where things are on the screens. Every screen is laid out in units of a 1024 x 768 screen
//! (the DS+'s panels), then scaled to the panel or the preview window by its `View`.

use crate::art::{self, Slot, CART_H, CART_W};
use crate::rom::Console;
use sdl2::rect::Rect;

/// A screen's size in layout units.
pub const SW: f32 = 1024.0;
pub const SH: f32 = 768.0;

/// The DS carts on the shelf: the centre one's width, and the pitch between them.
pub const CART_SIZE: f32 = 400.0;
pub const STEP: f32 = 330.0;
/// Slot-2 carts (GBA) on the shelf: their width, and the pitch between them.
const SLOT2_SIZE: f32 = 560.0;
const SLOT2_STEP: f32 = 470.0;
/// Game Boy carts, as wide as a GBA's and nearly twice as tall: narrower on the shelf, so the
/// tabs above and the slot below stay clear.
const GB_SIZE: f32 = 420.0;
const GB_STEP: f32 = 360.0;
/// The centre of the carts on the shelf.
pub const CART_Y: f32 = 318.0;
/// The status bar along the bottom of the touch screen, with the slot in its top edge.
pub const BAR_H: f32 = 96.0;

/// The cover grid: columns, whole rows on screen, a cover's size, and the pitch between covers.
pub const COLS: usize = 5;
pub const GRID_ROWS: usize = 3;
pub const GRID_COVER: (f32, f32) = (180.0, 164.0);
pub const GRID_STEP: (f32, f32) = (196.0, 190.0);
pub const GRID_AT: (f32, f32) = (30.0, 62.0);

/// The list: where its rows start, and the room they have down to the hint line.
pub const LIST_AT: (f32, f32) = (30.0, 62.0);
pub const LIST_W: f32 = SW - 2.0 * LIST_AT.0;
const LIST_ROOM: f32 = 554.0;
/// A row of the list with names only, and one with a small box beside the name.
const LIST_ROW: f32 = 50.0;
const LIST_COVER_ROW: f32 = 92.0;
/// The space a small box has at the start of its row.
pub const LIST_COVER: (f32, f32) = (96.0, 80.0);

/// A list's row height and how many whole rows are on screen, with covers or without.
pub fn list_rows(covers: bool) -> (f32, usize) {
    let h = if covers { LIST_COVER_ROW } else { LIST_ROW };
    (h, (LIST_ROOM / h) as usize)
}

/// The top of a place in the list, `scroll` rows down.
pub fn list_row(place: usize, scroll: f32, covers: bool) -> f32 {
    LIST_AT.1 + (place as f32 - scroll) * list_rows(covers).0
}

/// The place in the list under a touch, which may be past the last game.
pub fn list_hit(at: (f32, f32), scroll: f32, covers: bool) -> Option<usize> {
    let (h, rows) = list_rows(covers);
    let row = (at.1 - LIST_AT.1) / h;
    ((LIST_AT.0..LIST_AT.0 + LIST_W).contains(&at.0) && (0.0..rows as f32).contains(&row))
        .then_some((row + scroll) as usize)
}

/// The page tabs (DS | GBA | ...) at the top left of the touch screen: where, and each one's size.
pub const TAB: (f32, f32, f32, f32) = (28.0, 10.0, 84.0, 40.0);
/// Stars and hearts are drawn this size.
pub const ICON: f32 = 26.0;
/// A touch that moves less than this is a tap.
pub const TAP_SLOP: f32 = 14.0;
/// How far down the centre cart has to be pulled to go into the slot.
pub const PULL_TO_INSERT: f32 = 140.0;

/// Where one screen is: its offset and scale on its canvas, and which canvas it is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub ox: f32,
    pub oy: f32,
    pub s: f32,
    /// Which canvas the screen is on.
    pub c: usize,
}

impl View {
    /// A rectangle in layout units, in the canvas's pixels.
    /// As [`View::rect`], not rounded to whole pixels: for something moving slowly, so it
    /// glides rather than steps (drawn with `copy_f`, filtered).
    pub fn frect(&self, x: f32, y: f32, w: f32, h: f32) -> sdl2::rect::FRect {
        sdl2::rect::FRect::new(
            self.ox + x * self.s,
            self.oy + y * self.s,
            w * self.s,
            h * self.s,
        )
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (px, py) = (
            (self.ox + x * self.s).round() as i32,
            (self.oy + y * self.s).round() as i32,
        );
        Rect::new(
            px,
            py,
            (w * self.s).round().max(1.0) as u32,
            (h * self.s).round().max(1.0) as u32,
        )
    }

    /// A point on the canvas, in layout units.
    pub fn unscale(&self, x: i32, y: i32) -> (f32, f32) {
        ((x as f32 - self.ox) / self.s, (y as f32 - self.oy) / self.s)
    }
}

/// A cart on the shelf: width and height in screen units, and the pitch between carts.
pub fn cart_geometry(console: Console) -> (f32, f32, f32) {
    let (w, h) = art::cart_size(console);
    let width = match console {
        Console::Gb
        | Console::Gbc
        | Console::Nes
        | Console::GameGear
        | Console::Lynx
        | Console::PlayStation
        | Console::PcEngineCd => (GB_SIZE, GB_STEP),
        _ => (SLOT2_SIZE, SLOT2_STEP),
    };
    match art::slot(console) {
        Slot::One => (CART_SIZE, CART_SIZE * CART_H as f32 / CART_W as f32, STEP),
        Slot::Two => (width.0, width.0 * h as f32 / w as f32, width.1),
    }
}

/// The heading right of the tabs ("9 / 251 · Recently played"): its right edge (lined up with
/// the status bar's clock), the gap kept between it and the tabs, and the most room it takes.
pub const HEADING_RIGHT: f32 = SW - 40.0;
const HEADING_GAP: f32 = 24.0;
pub const HEADING_MOST: f32 = 420.0;
/// The tab strip is never narrower than this many tabs.
const STRIP_LEAST: f32 = 3.0;
/// Where a strip that runs over fades out, and a chevron says there is more: tapping it there
/// scrolls the strip.
pub const STRIP_FADE: f32 = 40.0;

/// The page tabs, laid out for a frame: they keep their size, and when there are more than
/// fit beside the heading, the strip they are in scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    /// Its left edge and width.
    pub x: f32,
    pub width: f32,
    /// How far along it is scrolled.
    pub scroll: f32,
    pub tabs: usize,
}

/// A touch on the tab strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StripHit {
    Tab(usize),
    /// On the faded edge at the left or right, where more tabs are.
    Left,
    Right,
}

impl Strip {
    /// `tabs` tabs beside a heading `heading` wide.
    pub fn new(tabs: usize, heading: f32, scroll: f32) -> Strip {
        let x = TAB.0;
        let room = HEADING_RIGHT - heading.min(HEADING_MOST) - HEADING_GAP - x;
        let width = room.max(STRIP_LEAST * TAB.2).min(tabs as f32 * TAB.2);
        let mut strip = Strip {
            x,
            width,
            scroll: 0.0,
            tabs,
        };
        strip.scroll = scroll.clamp(0.0, strip.most());
        strip
    }

    /// How far it can scroll.
    pub fn most(&self) -> f32 {
        (self.tabs as f32 * TAB.2 - self.width).max(0.0)
    }

    /// Whether there are tabs out of sight to the left, and to the right.
    pub fn more(&self) -> (bool, bool) {
        (self.scroll > 0.5, self.scroll < self.most() - 0.5)
    }

    /// The scroll that puts tab `i` in the middle (as near as the ends allow).
    pub fn centred(&self, i: usize) -> f32 {
        ((i as f32 + 0.5) * TAB.2 - self.width / 2.0).clamp(0.0, self.most())
    }

    /// Tab `i`'s rectangle, where it is drawn (it may be partly or wholly out of the strip).
    pub fn tab(&self, i: usize) -> (f32, f32, f32, f32) {
        (self.x + i as f32 * TAB.2 - self.scroll, TAB.1, TAB.2, TAB.3)
    }

    /// The room left for the heading, from the strip's right edge to the clock's.
    pub fn heading_room(&self) -> f32 {
        HEADING_RIGHT - (self.x + self.width + HEADING_GAP)
    }

    /// What a touch is on: a tab, or a faded edge (which scrolls the strip), with a little slack
    /// round the strip, as it is small for a finger.
    pub fn hit(&self, x: f32, y: f32) -> Option<StripHit> {
        let (_, ty, _, h) = TAB;
        let (left, right) = (self.x, self.x + self.width);
        if y < ty - 10.0 || y > ty + h + 14.0 || x < left - 14.0 || x > right + 10.0 {
            return None;
        }
        let (back, on) = self.more();
        if back && x < left + STRIP_FADE {
            return Some(StripHit::Left);
        }
        if on && x > right - STRIP_FADE {
            return Some(StripHit::Right);
        }
        let x = x.clamp(left, right - 1.0);
        let i = ((x - left + self.scroll) / TAB.2) as usize;
        Some(StripHit::Tab(i.min(self.tabs.saturating_sub(1))))
    }
}

/// The system picker (Select on the shelf): a band across the touch screen ("blade") for each
/// page, stacked down it. The chosen one is tall, in the middle; the others are slices, the
/// thinner the further they are from it, as if the stack curved away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blades {
    /// The chosen blade's height, a neighbour's, and how much each slice further on keeps.
    pub tall: f32,
    pub slice: f32,
    pub fall: f32,
    /// How far the edges between blades lean, in degrees (0: square).
    pub lean: f32,
}

/// Where the blades are, down the touch screen: between the tabs and the hint line.
pub const BLADES_TOP: f32 = 56.0;
pub const BLADES_BOTTOM: f32 = 620.0;
/// The dark gap between two blades.
pub const BLADE_GAP: f32 = 4.0;

impl Blades {
    /// The picker's look: a tall chosen blade, and slices that thin quickly away from it,
    /// as if the stack curved away steeply.
    pub const DEEP: Blades = Blades {
        tall: 250.0,
        slice: 56.0,
        fall: 0.68,
        lean: 0.0,
    };

    /// How far a finger moves the stack by one blade, near the middle.
    pub fn drag_step(&self) -> f32 {
        (self.tall + self.slice) / 2.0
    }

    /// The height of a blade `d` places from the chosen one (between, while the stack glides).
    pub fn height(&self, d: f32) -> f32 {
        let a = d.abs();
        if a < 1.0 {
            self.tall + (self.slice - self.tall) * a
        } else {
            self.slice * self.fall.powf(a - 1.0)
        }
    }

    /// Blade `i`'s top and height, with the stack at `pos` (the chosen blade's place, between
    /// two while it glides): the chosen one in the middle, each touching the next.
    pub fn blade(&self, i: usize, pos: f32) -> (f32, f32) {
        // Blade k spans places k - 0.5 to k + 0.5, as tall as its height there; the middle of
        // the space is at place `pos`. Its top is the middle, less the blades (and parts of
        // blades) between that place and its own top.
        let height = |k: i32| self.height(k as f32 - pos);
        let from = i as f32 - 0.5;
        let (lo, hi) = (from.min(pos), from.max(pos));
        let span: f32 = (lo.floor() as i32 - 1..=hi.ceil() as i32 + 1)
            .map(|k| {
                let overlap = (hi.min(k as f32 + 0.5) - lo.max(k as f32 - 0.5)).max(0.0);
                overlap * height(k)
            })
            .sum();
        let top = (BLADES_TOP + BLADES_BOTTOM) / 2.0 + if from < pos { -span } else { span };
        (top, height(i as i32))
    }

    /// The blade under a touch, of `n`, with the stack at `pos`.
    pub fn hit(&self, y: f32, n: usize, pos: f32) -> Option<usize> {
        if !(BLADES_TOP..BLADES_BOTTOM).contains(&y) {
            return None;
        }
        (0..n).find(|&i| {
            let (top, h) = self.blade(i, pos);
            (top..top + h).contains(&y)
        })
    }
}

/// Top-left corner of a place in the cover grid.
pub fn grid_cell(place: usize, scroll: f32) -> (f32, f32) {
    (
        GRID_AT.0 + (place % COLS) as f32 * GRID_STEP.0,
        GRID_AT.1 + ((place / COLS) as f32 - scroll) * GRID_STEP.1,
    )
}

/// The place in the cover grid under a touch, which may be past the last game.
pub fn grid_hit(at: (f32, f32), scroll: f32) -> Option<usize> {
    let (col, row) = (
        (at.0 - GRID_AT.0) / GRID_STEP.0,
        (at.1 - GRID_AT.1) / GRID_STEP.1 + scroll,
    );
    (col >= 0.0 && (col as usize) < COLS && row >= 0.0).then(|| row as usize * COLS + col as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_strip_that_fits() {
        let strip = Strip::new(2, 200.0, 50.0);
        assert_eq!(
            (strip.width, strip.scroll, strip.more()),
            (2.0 * TAB.2, 0.0, (false, false))
        );
        assert_eq!(
            strip.hit(TAB.0 + 10.0, TAB.1 + 10.0),
            Some(StripHit::Tab(0))
        );
        assert_eq!(
            strip.hit(TAB.0 + TAB.2 + 10.0, TAB.1 + 10.0),
            Some(StripHit::Tab(1))
        );
        // Just right of the last tab still hits it; well below the tabs does not.
        assert_eq!(
            strip.hit(TAB.0 + 2.0 * TAB.2 + 5.0, TAB.1),
            Some(StripHit::Tab(1))
        );
        assert_eq!(strip.hit(TAB.0, 200.0), None);
        assert!(strip.heading_room() >= 200.0);
    }

    #[test]
    fn a_strip_that_runs_over_scrolls() {
        let heading = 240.0;
        let mut strip = Strip::new(12, heading, 0.0);
        assert!(strip.width < 12.0 * TAB.2 && strip.heading_room() >= heading);
        assert_eq!(strip.more(), (false, true));
        // Centred on the seventh tab, there is more both ways, and the tabs are hit where they
        // are drawn.
        strip.scroll = strip.centred(6);
        let (x, _, w, _) = strip.tab(6);
        assert!(((x + w / 2.0) - (strip.x + strip.width / 2.0)).abs() < 1.0);
        assert_eq!(strip.more(), (true, true));
        assert_eq!(strip.hit(x + 5.0, TAB.1 + 5.0), Some(StripHit::Tab(6)));
        assert_eq!(strip.hit(strip.x + 5.0, TAB.1 + 5.0), Some(StripHit::Left));
        assert_eq!(
            strip.hit(strip.x + strip.width - 5.0, TAB.1 + 5.0),
            Some(StripHit::Right)
        );
        // The last tab centres as near as the end allows: flush right.
        assert_eq!(strip.centred(11), strip.most());
        // However long the heading, the strip keeps three tabs.
        assert!(Strip::new(12, 2000.0, 0.0).width >= 3.0 * TAB.2);
    }

    #[test]
    fn blades_stack_and_glide() {
        let blades = Blades::DEEP;
        let mid = (BLADES_TOP + BLADES_BOTTOM) / 2.0;
        // The chosen one is tall and in the middle; its neighbours touch it, thinner.
        let (top, h) = blades.blade(5, 5.0);
        assert!((top + h / 2.0 - mid).abs() < 0.5 && h == blades.tall);
        let (below, hb) = blades.blade(6, 5.0);
        assert!((below - (top + h)).abs() < 1.0 && hb < h);
        let (above, ha) = blades.blade(4, 5.0);
        assert!((above + ha - top).abs() < 1.0);
        assert!(blades.blade(8, 5.0).1 < blades.blade(7, 5.0).1);
        // Half way through a glide, the two share the middle.
        let (a, ah) = blades.blade(5, 5.5);
        let (b, _) = blades.blade(6, 5.5);
        assert!((a + ah - b).abs() < 1.0 && (b - mid).abs() < 1.0);
        assert_eq!(blades.hit(mid, 11, 5.0), Some(5));
        assert_eq!(blades.hit(below + 2.0, 11, 5.0), Some(6));
        assert_eq!(blades.hit(10.0, 11, 5.0), None);
    }

    #[test]
    fn grid() {
        for place in [0, 4, 5, 17] {
            let (x, y) = grid_cell(place, 1.0);
            assert_eq!(grid_hit((x + 1.0, y + 1.0), 1.0), Some(place));
        }
        // Left of the first column, or right of the last.
        assert_eq!(grid_hit((GRID_AT.0 - 1.0, 100.0), 0.0), None);
        assert_eq!(
            grid_hit((GRID_AT.0 + COLS as f32 * GRID_STEP.0 + 1.0, 100.0), 0.0),
            None
        );
    }

    #[test]
    fn list() {
        for covers in [false, true] {
            let (h, rows) = list_rows(covers);
            // Every row on screen ends above the hint line.
            assert!(LIST_AT.1 + rows as f32 * h <= 620.0);
            for place in [3, 4, 8] {
                let y = list_row(place, 3.0, covers);
                assert_eq!(list_hit((100.0, y + 1.0), 3.0, covers), Some(place));
            }
            assert_eq!(list_hit((100.0, LIST_AT.1 - 1.0), 0.0, covers), None);
            assert_eq!(
                list_hit((100.0, LIST_AT.1 + rows as f32 * h + 1.0), 0.0, covers),
                None
            );
        }
        assert_eq!(list_rows(false).1, 11);
        assert_eq!(list_rows(true).1, 6);
    }

    #[test]
    fn views() {
        let half = View {
            ox: 0.0,
            oy: 384.0,
            s: 0.5,
            c: 0,
        };
        assert_eq!(
            half.rect(100.0, 100.0, 200.0, 50.0),
            Rect::new(50, 434, 100, 25)
        );
        assert_eq!(half.unscale(50, 434), (100.0, 100.0));
    }
}
