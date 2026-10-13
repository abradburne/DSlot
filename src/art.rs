//! Builds each game's cart and box. Real artwork wins when it exists; otherwise both are
//! generated from the ROM's own icon, title and game code.

use crate::gfx::{load_png, Fonts, Img, Rgba};
use crate::rom::{icon_colour, Game};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub const CART_W: usize = 448;
pub const CART_H: usize = 480;
/// A DS case seen from the front, 137 x 125 mm, and the plastic showing around the cover.
pub const CASE_SIZE: (usize, usize) = (482, 440);
const CASE_EDGE: usize = 7;
const CASE_RADIUS: f32 = 10.0;
/// Plastic and the lighter rim on its edge: the black case, then the clear one.
const CASE_BLACK: [Rgba; 2] = [[24, 26, 31, 255], [70, 74, 84, 255]];
const CASE_CLEAR: [Rgba; 2] = [[198, 204, 212, 225], [244, 246, 250, 245]];

/// The cart's plastic is drawn on a 336 x 360 grid with the sticker's window at x, y, w, h.
/// Those are the proportions GameTDB uses for its DS cart and label images, so labels made
/// for one fit the other.
const FRAME_SIZE: (f32, f32) = (336.0, 360.0);
const FRAME_STICKER: (f32, f32, f32, f32) = (32.0, 20.0, 272.0, 296.0);

/// A DS card's front, laid out on that grid. The body is a pixel narrower than the grid on each
/// side. A groove runs round its lower edges, setting off a narrow lip: it starts at a ledge on
/// the left edge, follows the big bottom-left corner, runs along the bottom and turns up the
/// right side to a second ledge.
mod card {
    pub const BODY: (f32, f32, f32, f32) = (1.0, 0.0, 334.0, 360.0);
    /// Corner radii: top-left, top-right, bottom-right, bottom-left. The last is much the roundest.
    pub const CORNERS: [f32; 4] = [9.0, 9.0, 9.0, 34.0];

    /// The groove. Left: ledge height and the x it runs down at. Its turn: centre and radius.
    /// Bottom: its height. Right: the x it runs up at, and that ledge's height.
    pub const LEFT: (f32, f32) = (308.7, 5.5);
    pub const TURN: (f32, f32, f32) = (35.6, 323.4, 30.1);
    pub const BOTTOM: f32 = 353.6;
    pub const RIGHT: (f32, f32) = (330.2, 329.3);

    /// The well the sticker sits in, its corner radius, and how much of its bottom-left
    /// corner is cut off at 45 degrees. The window inside it is cut the same way.
    pub const WELL: (f32, f32, f32, f32) = (24.5, 12.5, 286.0, 310.0);
    pub const WELL_RADIUS: f32 = 11.0;
    pub const WELL_CUT: f32 = 21.5;
    pub const WINDOW: (f32, f32, f32, f32) = (31.5, 19.5, 272.0, 296.5);
    pub const WINDOW_RADIUS: f32 = 12.0;
    pub const WINDOW_CUT: f32 = 23.5;
    /// The arrow pointing into the slot: centre, top, width, height.
    pub const ARROW: (f32, f32, f32, f32) = (167.5, 327.2, 40.0, 14.8);

    /// Greys, as brightness; the plastic has a faint green cast (see `Shell::load`).
    pub const FACE: f32 = 72.0;
    pub const WELL_FACE: f32 = 56.0;
    pub const WELL_RIM: f32 = 46.0;
    pub const ARROW_FACE: f32 = 58.0;
    pub const GROOVE: f32 = 22.0;
    /// The lip catches the light on the left and is in shade along the bottom.
    pub const LIP: (f32, f32, f32) = (84.0, 67.0, 71.0);
}

pub struct Shell {
    frame: Img,
    /// Sticker window at cart size, grown by a pixel so nothing shows through at the edge.
    sticker: (i32, i32, i32, i32),
}

/// Signed distance to a rounded rectangle, negative inside. Radii as in `card::CORNERS`.
fn rounded(x: f32, y: f32, rect: (f32, f32, f32, f32), r: [f32; 4]) -> f32 {
    let (dx, dy) = (x - rect.0 - rect.2 / 2.0, y - rect.1 - rect.3 / 2.0);
    let rad = match (dx < 0.0, dy < 0.0) {
        (true, true) => r[0],
        (false, true) => r[1],
        (false, false) => r[2],
        (true, false) => r[3],
    };
    let (qx, qy) = (dx.abs() - rect.2 / 2.0 + rad, dy.abs() - rect.3 / 2.0 + rad);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad
}

/// The same with the bottom-left corner cut off at 45 degrees, `cut` along each side.
fn clipped(x: f32, y: f32, rect: (f32, f32, f32, f32), r: f32, cut: f32) -> f32 {
    let corner = (cut - (x - rect.0) - (rect.1 + rect.3 - y)) / std::f32::consts::SQRT_2;
    rounded(x, y, rect, [r; 4]).max(corner)
}

/// Distance from a point to a line segment.
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    (x - a.0 - vx * t).hypot(y - a.1 - vy * t)
}

/// Distance to the groove, and how far along it the nearest point is: 0 on the left side,
/// 1 along the bottom, 2 up the right.
fn groove(x: f32, y: f32) -> (f32, f32) {
    use card::*;
    let right = BODY.0 + BODY.2;
    let (cx, cy, r) = TURN;
    let mut best = (f32::MAX, 0.0);
    let mut take = |d: f32, along: f32| {
        if d < best.0 {
            best = (d, along);
        }
    };
    take(segment(x, y, (BODY.0, LEFT.0), (LEFT.1, LEFT.0)), 0.0);
    take(segment(x, y, (LEFT.1, LEFT.0), (LEFT.1, cy)), 0.0);
    // The quarter turn, where the point is in its quadrant.
    if x <= cx && y >= cy {
        let turned = (y - cy).atan2(cx - x) / std::f32::consts::FRAC_PI_2;
        take(((x - cx).hypot(y - cy) - r).abs(), turned);
    }
    take(segment(x, y, (cx, BOTTOM), (RIGHT.0 - 2.0, BOTTOM)), 1.0);
    take(segment(x, y, (RIGHT.0 - 2.0, BOTTOM), (RIGHT.0, BOTTOM - 2.0)), 1.5);
    take(segment(x, y, (RIGHT.0, BOTTOM - 2.0), (RIGHT.0, RIGHT.1)), 2.0);
    take(segment(x, y, (RIGHT.0, RIGHT.1), (right, RIGHT.1)), 2.0);
    best
}

/// Whether a point is on the lip, outside the groove.
fn on_lip(x: f32, y: f32) -> bool {
    use card::*;
    let (cx, cy, r) = TURN;
    (x < LEFT.1 && y > LEFT.0) || y > BOTTOM || (x > RIGHT.0 && y > RIGHT.1) || (x < cx && y > cy && (x - cx).hypot(y - cy) > r)
}

impl Shell {
    pub fn load() -> Shell {
        use card::*;
        let (fx, fy) = (CART_W as f32 / FRAME_SIZE.0, CART_H as f32 / FRAME_SIZE.1);
        let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
        // Distances are in grid units; this turns one into coverage at the edge of a shape.
        let cover = |d: f32| (0.5 - d * fx).clamp(0.0, 1.0);
        // A soft line of the given half-width centred on distance zero.
        let line = |d: f32, half: f32| (1.0 - d.abs() / half).clamp(0.0, 1.0);

        let mut frame = Img::new(CART_W, CART_H);
        for py in 0..CART_H {
            for px in 0..CART_W {
                let (x, y) = ((px as f32 + 0.5) / fx, (py as f32 + 0.5) / fy);
                let outline = rounded(x, y, BODY, CORNERS);

                // The face, lit from the top left: a shade darker toward the right.
                let mut c = FACE * (1.02 - 0.05 * x / FRAME_SIZE.0);
                let (dist, along) = groove(x, y);
                if on_lip(x, y) {
                    c = if along <= 1.0 { mix(LIP.0, LIP.1, along) } else { mix(LIP.1, LIP.2, along - 1.0) };
                } else if along < 1.0 {
                    // The face rises a little brighter just inside the groove on the lit side.
                    c += 6.0 * line(dist - 2.6, 1.3) * (1.0 - along);
                }
                // The groove is deepest on the sides and shallower along the bottom.
                let depth = if along <= 1.0 { mix(1.0, 0.72, along) } else { mix(0.72, 1.0, (along - 1.0) * 2.0) };
                c = mix(c, GROOVE, depth * line(dist, 1.25));

                // The well: a bright edge on the face around it, a dark rim just inside.
                let well = clipped(x, y, WELL, WELL_RADIUS, WELL_CUT);
                c += 5.0 * line(well - 1.8, 1.6);
                let rim = if y < WELL.1 + WELL_RADIUS { 0.35 } else { 1.0 };
                let inside = mix(WELL_FACE, WELL_RIM, rim * line(well + 1.3, 1.5));
                c = mix(c, inside, cover(well));

                let (ax, ay) = ((x - ARROW.0).abs(), y - ARROW.1);
                let arrow = (ax - ARROW.2 / 2.0 * (1.0 - ay / ARROW.3)).max(-ay);
                c = mix(c, ARROW_FACE, cover(arrow * 0.8));

                // The outer edge falls away into shadow, except on the left where it is lit.
                let edge = -outline;
                if x < BODY.0 + 4.0 && y > CORNERS[0] && y < TURN.1 {
                    c *= if edge < 1.0 { 0.84 } else { mix(1.08, 1.0, (edge - 2.2) / 1.2) };
                } else {
                    c *= mix(0.42, 1.0, (edge + 0.2) / 2.6);
                }

                // Matte plastic: a fine grain.
                let grain = (px as u32).wrapping_mul(0x9E3779B1).wrapping_add((py as u32).wrapping_mul(0x85EBCA6B));
                let grain = ((grain ^ (grain >> 15)).wrapping_mul(0x2C1B3C6D) >> 28) as f32 - 7.5;
                c *= 1.0 + grain * 0.004;

                let window = clipped(x, y, WINDOW, WINDOW_RADIUS, WINDOW_CUT);
                let alpha = cover(outline) * (1.0 - cover(window));
                let i = (py * CART_W + px) * 4;
                // Slightly more green and blue than red.
                for (k, tint) in [0.97, 1.02, 1.02].into_iter().enumerate() {
                    frame.px[i + k] = (c * tint).clamp(0.0, 255.0) as u8;
                }
                frame.px[i + 3] = (alpha * 255.0) as u8;
            }
        }

        let (x, y, w, h) = FRAME_STICKER;
        let sticker = ((x * fx) as i32 - 1, (y * fy) as i32 - 1, (w * fx).ceil() as i32 + 2, (h * fy).ceil() as i32 + 2);
        Shell { frame, sticker }
    }
}

/// `Carts/<rom>.png` is a finished cart and is drawn as it is. `Labels/<rom>.png` is the
/// sticker alone and goes behind the frame. With neither, a sticker is printed from the ROM.
pub fn cart(shell: &Shell, game: &Game, fonts: &Fonts) -> Img {
    let mut img = Img::new(CART_W, CART_H);
    let full = |src: &Img| (0.0, 0.0, src.w as f32, src.h as f32);
    if let Some(c) = load_png(&game.art("Carts")) {
        img.blit(&c, full(&c), (0, 0, CART_W as i32, CART_H as i32), false);
        return img;
    }
    let sticker = load_png(&game.art("Labels")).unwrap_or_else(|| printed(game, fonts, shell.sticker.2, shell.sticker.3));
    img.blit(&sticker, full(&sticker), shell.sticker, false);
    img.blit(&shell.frame, full(&shell.frame), (0, 0, CART_W as i32, CART_H as i32), false);
    img
}

/// A sticker made from what the ROM carries: title on the band, icon as the artwork, game code below.
fn printed(game: &Game, fonts: &Fonts, w: i32, h: i32) -> Img {
    const INK: Rgba = [48, 50, 56, 255];
    let mut img = Img::new(w as usize, h as usize);
    img.rrect(0.0, 0.0, w as f32, h as f32, [w as f32 * 0.05; 4], [243, 242, 238, 255]);
    let (band, strip) = ((h as f32 * 0.18) as i32, (h as f32 * 0.15) as i32);
    let art_h = h - band - strip;

    let size = fonts.fit(game.name(), band as f32 * 0.52, 14.0, w as f32 - 36.0, true);
    let t = fonts.render(game.name(), size, true);
    img.stamp(&t, (w - t.w as i32) / 2, (band - t.h as i32) / 2 + 2, INK);

    let [r, g, b] = game.icon.as_ref().map(icon_colour).unwrap_or([70, 80, 100]);
    img.rrect(0.0, band as f32, w as f32, art_h as f32, [0.0; 4], [r / 3 + 20, g / 3 + 22, b / 3 + 28, 255]);
    if let Some(icon) = &game.icon {
        let k = ((w.min(art_h) as f32 * 0.82) as i32 / 32).max(1) * 32;
        img.blit(icon, (0.0, 0.0, 32.0, 32.0), ((w - k) / 2, band + (art_h - k) / 2, k, k), true);
    }

    let t = fonts.render(&game.serial(), h as f32 * 0.055, true);
    img.stamp(&t, w - t.w as i32 - 16, h - strip + (strip - t.h as i32) / 2 - 2, INK);
    img
}

/// Box front: the cover in a DS case, the plastic showing as a rounded frame around it.
/// Every box is the same shape, as on a real shelf.
pub fn boxart(game: &Game, fonts: &Fonts, black: bool) -> Img {
    let cover = cover(game, fonts);
    let ((w, h), e) = (CASE_SIZE, CASE_EDGE as i32);
    let [plastic, rim] = if black { CASE_BLACK } else { CASE_CLEAR };
    let mut img = Img::new(w, h);
    // A lighter rim a pixel wide catches the light on the case's edge.
    img.rrect(0.0, 0.0, w as f32, h as f32, [CASE_RADIUS; 4], rim);
    img.rrect(1.0, 1.0, w as f32 - 2.0, h as f32 - 2.0, [CASE_RADIUS - 1.0; 4], plastic);
    img.blit(&cover, (0.0, 0.0, cover.w as f32, cover.h as f32), (e, e, cover.w as i32, cover.h as i32), false);
    img
}

/// The box at grid size. Reading and scaling a cover is the slow part of showing it, so the
/// result is kept in `cache` and reused until the cover (or, without one, the ROM) changes.
pub fn thumb(game: &Game, fonts: &Fonts, black: bool, size: (usize, usize), cache: &Path) -> Img {
    let (w, h) = size;
    let art = game.art("Imgs");
    let source = fs::metadata(&art).or_else(|_| fs::metadata(&game.path)).ok();
    let stamp = source.map_or([0; 2], |m| [m.len(), m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())]);
    let head: Vec<u8> = stamp.iter().flat_map(|v| v.to_le_bytes()).collect();

    // FNV-1a over everything the picture depends on bar the source's contents.
    let key = format!("{}|{black}|{w}x{h}", game.path.display());
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let file = cache.join(format!("{hash:016x}.thumb"));
    if let Ok(bytes) = fs::read(&file) {
        if bytes.len() == head.len() + w * h * 4 && bytes.starts_with(&head) {
            return Img { w, h, px: bytes[head.len()..].to_vec() };
        }
    }

    let img = boxart(game, fonts, black).shrink(w, h);
    let _ = fs::create_dir_all(cache);
    let _ = fs::write(&file, [head.as_slice(), &img.px].concat());
    img
}

/// `Imgs/<rom>.png` scaled to fill the case and cropped about its centre, or a generated cover.
fn cover(game: &Game, fonts: &Fonts) -> Img {
    let (w, h) = (CASE_SIZE.0 - 2 * CASE_EDGE, CASE_SIZE.1 - 2 * CASE_EDGE);
    if let Some(src) = load_png(&game.art("Imgs")) {
        let k = (src.w as f32 / w as f32).min(src.h as f32 / h as f32);
        let (sw, sh) = (w as f32 * k, h as f32 * k);
        let mut img = Img::new(w, h);
        img.blit(&src, ((src.w as f32 - sw) / 2.0, (src.h as f32 - sh) / 2.0, sw, sh), (0, 0, w as i32, h as i32), false);
        return img;
    }

    let strip = 48;
    let [r, g, b] = game.icon.as_ref().map(icon_colour).unwrap_or([70, 80, 100]);
    let mut img = Img::new(w, h);
    for y in 0..h {
        // Lighter at the top, darker at the bottom.
        let k = 0.95 - 0.55 * y as f32 / h as f32;
        let c = [(r as f32 * k) as u8 + 12, (g as f32 * k) as u8 + 12, (b as f32 * k) as u8 + 16, 255];
        for x in strip..w {
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&c);
        }
        for x in 0..strip {
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&[22, 24, 29, 255]);
        }
    }
    let panel = (w - strip) as i32;
    if let Some(icon) = &game.icon {
        img.blit(icon, (0.0, 0.0, 32.0, 32.0), (strip as i32 + (panel - 224) / 2, 44, 224, 224), true);
    }
    let size = fonts.fit(game.name(), 40.0, 18.0, panel as f32 - 40.0, true);
    let t = fonts.render(game.name(), size, true);
    img.stamp(&t, strip as i32 + (panel - t.w as i32) / 2, 312, [255, 255, 255, 255]);
    if let Some(maker) = game.title.last().filter(|_| game.title.len() > 1) {
        let t = fonts.render(maker, 20.0, false);
        img.stamp(&t, strip as i32 + (panel - t.w as i32) / 2, 364, [255, 255, 255, 190]);
    }
    img
}
