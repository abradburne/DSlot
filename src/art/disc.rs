//! A CD game drawn as its disc, for the PlayStation's and PC Engine CD's shelves (and any CD
//! system after them), the way the carts are drawn for the cartridge systems: in code, to a
//! real disc's proportions. Those are the CD's published dimensions, not any one disc's: 120 mm
//! across, a 15 mm spindle hole, the clear clamping area out to 33 mm, the bare mirror band to
//! about 46 mm, and the printed face from there almost to the rim.
//!
//! The printed face is the game's own art: `Labels/<rom>.png` where there is one (a disc's
//! print, square), else its box art filling the face, else its name on a colour field. A
//! PlayStation disc shows its black side's colour where it isn't printed, as one does; the
//! others are silver. A game of several discs (an .m3u) carries a badge saying how many.
//!
//! A PSP game is drawn as its UMD: the 60 mm disc in its 64 x 65 mm shell, to the shell's
//! proportions (measured from a photograph of one). The shell's top is a half circle round the
//! disc and its bottom an arc of 68 mm radius, the sides straight between them and 5 mm corners
//! rounded into the arc; it is milky clear plastic, the disc's print seen through it (the
//! game's label or box art), the hub a white cap with "UMD" on it, the PSP's mark in the bottom
//! left corner, the shell's notch in the bottom right and its yellow arrow at the top left.

use crate::gfx::{Fonts, Img, Rgba};
use crate::rom::{Console, Game};

/// A disc's size on the shelf, in pixels.
pub const SIZE: (usize, usize) = (480, 480);

/// The radii, as fractions of the disc's.
const HOLE: f32 = 7.5 / 60.0;
const HUB: f32 = 16.5 / 60.0;
const STACKING_RING: f32 = 0.2;
const MIRROR: f32 = 23.0 / 60.0;
const RIM: f32 = 0.985;

/// Whether `console`'s games are discs.
pub fn is_disc(console: Console) -> bool {
    matches!(
        console,
        Console::PlayStation | Console::Psp | Console::PcEngineCd
    )
}

/// How many discs `game` is: an .m3u lists them, one a line; anything else is one.
pub fn discs(game: &Game) -> usize {
    let m3u = game
        .path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("m3u"));
    if !m3u {
        return 1;
    }
    std::fs::read_to_string(&game.path)
        .map(|t| {
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .count()
        })
        .unwrap_or(1)
        .max(1)
}

/// The printed face, square: the label, else the box art filling it, else the name on a field.
fn face(game: &Game, fonts: &Fonts, label: Option<&Img>, cover: Option<&Img>, side: usize) -> Img {
    let mut img = Img::new(side, side);
    if let Some(src) = label.or(cover) {
        let k = (src.w as f32).min(src.h as f32);
        img.blit(
            src,
            ((src.w as f32 - k) / 2.0, (src.h as f32 - k) / 2.0, k, k),
            (0, 0, side as i32, side as i32),
            false,
        );
        return img;
    }
    let [r, g, b] = super::name_colour(game);
    img.rrect(0.0, 0.0, side as f32, side as f32, [0.0; 4], [r, g, b, 255]);
    // The name across the lower half of the face, below the hub, as a disc's print often is.
    let s = side as f32;
    let width = s * 0.62;
    let size = fonts.fit(game.name(), s * 0.075, s * 0.035, width, true);
    let lines = fonts.wrap(game.name(), size, true, width, 2);
    let mut y = s * 0.69;
    for line in lines {
        let t = fonts.render(&line, size, true);
        img.stamp(
            &t,
            (side as i32 - t.w as i32) / 2,
            y as i32,
            [250, 250, 250, 255],
        );
        y += size * 1.15;
    }
    img
}

/// `game`'s disc: `label` its own print (`Labels/`), `cover` its box art.
pub fn disc(game: &Game, fonts: &Fonts, label: Option<&Img>, cover: Option<&Img>) -> Img {
    if game.console == Console::Psp {
        return umd(game, fonts, label, cover);
    }
    let (w, h) = SIZE;
    let face = face(game, fonts, label, cover, w);
    let dark = game.console == Console::PlayStation;
    let mut img = Img::new(w, h);
    let (cx, cy, radius) = (w as f32 / 2.0, h as f32 / 2.0, w as f32 / 2.0 - 1.0);
    // How far a pixel is inside an edge at radius `r` (in pixels), for smooth edges.
    let inside = |d: f32, r: f32| (r - d + 0.5).clamp(0.0, 1.0);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
            let d = x.hypot(y);
            let t = d / radius;
            let alpha = inside(d, radius) * (1.0 - inside(d, HOLE * radius));
            if alpha <= 0.0 {
                continue;
            }
            let angle = y.atan2(x);
            // The light across a disc: two bright sweeps opposite each other, and a faint
            // rainbow that turns with the angle.
            let sweep = (2.0 * angle - 0.9).cos() * 0.5 + 0.5;
            let rainbow = [
                (angle * 2.0).sin(),
                (angle * 2.0 + 2.1).sin(),
                (angle * 2.0 + 4.2).sin(),
            ];
            let silver = |base: f32, gloss: f32| -> [f32; 3] {
                let v = base + gloss * sweep;
                [0, 1, 2].map(|i| v + rainbow[i] * gloss * 0.35)
            };
            let (c, a): ([f32; 3], f32) = if t < HUB {
                // Clear plastic, the stacking ring a raised line in it.
                let ring = (1.0 - ((t - STACKING_RING) * radius / 1.5).abs()).max(0.0);
                let tint = if dark { 70.0 } else { 205.0 };
                ([tint + ring * 40.0; 3], 0.45 + ring * 0.3)
            } else if t < MIRROR {
                // The bare mirror band.
                let c = if dark {
                    silver(38.0, 50.0)
                } else {
                    silver(168.0, 70.0)
                };
                (c, 1.0)
            } else if t < RIM {
                // The printed face, with a little of the disc's sheen through it.
                let i = (py * w + px) * 4;
                let p = &face.px[i..i + 3];
                let gloss = 18.0 * sweep;
                ([0, 1, 2].map(|k| p[k] as f32 + gloss), 1.0)
            } else {
                // The rim.
                let c = if dark {
                    silver(30.0, 30.0)
                } else {
                    silver(150.0, 60.0)
                };
                (c, 1.0)
            };
            // The edges between the bands, a fine dark line, so they read as edges.
            let edge = [HUB, MIRROR, RIM]
                .iter()
                .map(|&b| (1.0 - ((t - b) * radius).abs()).max(0.0))
                .fold(0.0f32, f32::max);
            let c = c.map(|v| v * (1.0 - 0.35 * edge));
            let i = (py * w + px) * 4;
            for (k, v) in c.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (a * alpha * 255.0) as u8;
        }
    }
    let n = discs(game);
    if n > 1 {
        badge(&mut img, fonts, &format!("{n} discs"));
    }
    img
}

/// A UMD's dimensions, in millimetres: the shell 64 wide and 65 tall, its bottom corners'
/// radius, the disc 60 across, and its hub cap.
const UMD_SHELL: (f32, f32) = (64.0, 65.0);
const UMD_CORNER: f32 = 5.0;
/// The radius of the bottom's arc.
const UMD_ARC: f32 = 68.0;
const UMD_DISC: f32 = 60.0;
const UMD_HUB: f32 = 21.0;

/// `game`'s UMD: its print (`label`, else `cover`) on the disc seen through the shell.
fn umd(game: &Game, fonts: &Fonts, label: Option<&Img>, cover: Option<&Img>) -> Img {
    let (w, h) = SIZE;
    let mm = (h as f32 - 8.0) / UMD_SHELL.1;
    let (cx, top) = (w as f32 / 2.0, 4.0);
    let half = UMD_SHELL.0 / 2.0 * mm;
    // The disc's centre: the shell's half circle round it, 2 mm wider than the disc.
    let cy = top + half;
    let bottom = top + UMD_SHELL.1 * mm;
    let (disc_r, hub_r) = (UMD_DISC / 2.0 * mm, UMD_HUB / 2.0 * mm);
    let corner = UMD_CORNER * mm;
    // The print, square, over the disc.
    let side = (disc_r * 2.0) as usize;
    let print = face(game, fonts, label, cover, side);
    let (fx, fy) = (cx - disc_r, cy - disc_r);
    // Distance outside the shell (negative inside): the half circle above the disc's centre,
    // a rectangle with rounded bottom corners below it.
    // The bottom's arc, through the bottom's middle; how high it is where the sides meet it.
    let arc = UMD_ARC * mm;
    let arc_y = bottom - arc;
    let bow = arc - (arc * arc - (half - corner).powi(2)).sqrt() + corner * 0.6;
    let shell = |x: f32, y: f32| -> f32 {
        let ax = (x - cx).abs();
        if y <= cy {
            (x - cx).hypot(y - cy) - half
        } else {
            // The sides and the arc, met by a rounded corner.
            let side = ax - (half - corner);
            let under = (x - cx).hypot(y - arc_y) - arc + corner;
            side.max(0.0).hypot(under.max(0.0)) + side.max(under).min(0.0) - corner
        }
    };
    let mut img = Img::new(w, h);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let out = shell(x, y);
            let alpha = (0.5 - out).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let d = (x - cx).hypot(y - cy);
            // The shell, milky, a little brighter toward the top left.
            let light = 1.0 - 0.10 * ((x + y) / (w + h) as f32);
            let mut c = [214.0 * light, 217.0 * light, 222.0 * light];
            if d < disc_r {
                c = if d < hub_r {
                    // The hub's white cap, a grey ring round it.
                    let ring = (1.0 - (d - (hub_r - 3.0)).abs() / 2.5).max(0.0);
                    [246.0 - 70.0 * ring; 3]
                } else if d > disc_r - 0.9 * mm {
                    // The disc's edge through the shell.
                    [168.0; 3]
                } else {
                    // The print, a little milky under the shell.
                    let (ix, iy) = ((x - fx) as usize, (y - fy) as usize);
                    let i = (iy.min(side - 1) * side + ix.min(side - 1)) * 4;
                    let p = &print.px[i..i + 3];
                    [0, 1, 2].map(|k| p[k] as f32 * 0.92 + c[k] * 0.08)
                };
            }
            // The shell's edge, a fine darker line.
            let edge = (1.0 - (out + 2.0).abs() / 2.0).max(0.0);
            let c = c.map(|v| v * (1.0 - 0.3 * edge));
            let i = (py * w + px) * 4;
            for (k, v) in c.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    const INK: Rgba = [40, 42, 48, 255];
    // "UMD" on the hub's cap.
    let t = fonts.render("UMD", hub_r * 0.62, true);
    img.stamp(
        &t,
        (cx - t.w as f32 / 2.0) as i32,
        (cy - t.h as f32 / 2.0) as i32,
        INK,
    );
    // The PSP's mark in the bottom left corner, beside the disc.
    let t = fonts.render("PSP", 5.0 * mm, true);
    img.stamp(
        &t,
        (cx - half + 2.5 * mm) as i32,
        (bottom - bow - 2.6 * mm - t.h as f32) as i32,
        [110, 112, 120, 255],
    );
    // The shell's notch in the bottom right corner.
    img.rrect(
        cx + half - 9.0 * mm,
        bottom - bow - 4.4 * mm,
        6.0 * mm,
        2.2 * mm,
        [1.1 * mm; 4],
        [150, 153, 160, 255],
    );
    // The yellow arrow at the top left, pointing into the drive.
    let (ax, ay, size) = (cx - 0.64 * half, cy - 0.64 * half, 3.6 * mm);
    for py in (ay - size) as usize..(ay + size) as usize {
        for px in (ax - size) as usize..(ax + size) as usize {
            // A triangle pointing up and to the left, along the shell's diagonal.
            let (u, v) = (px as f32 + 0.5 - ax, py as f32 + 0.5 - ay);
            let (along, across) = (
                (u + v) * std::f32::consts::FRAC_1_SQRT_2,
                (v - u) * std::f32::consts::FRAC_1_SQRT_2,
            );
            let inside = along > -size * 0.7
                && along < size * 0.7
                && across.abs() < (along + size * 0.7) * 0.62;
            if inside {
                let i = (py * w + px) * 4;
                img.px[i..i + 4].copy_from_slice(&[250, 206, 50, 255]);
            }
        }
    }
    img
}

/// A small badge low on the disc: how many discs the game is.
fn badge(img: &mut Img, fonts: &Fonts, text: &str) {
    const INK: Rgba = [24, 26, 32, 255];
    let t = fonts.render(text, 30.0, true);
    let (bw, bh) = (t.w as f32 + 32.0, 44.0);
    let (bx, by) = ((img.w as f32 - bw) / 2.0, img.h as f32 * 0.78);
    img.rrect(bx, by, bw, bh, [bh / 2.0; 4], [255, 196, 60, 255]);
    img.stamp(
        &t,
        (bx + 16.0) as i32,
        (by + (bh - t.h as f32) / 2.0) as i32,
        INK,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn game(path: &std::path::Path, console: Console) -> Game {
        let mut g = crate::rom::read_one(path).unwrap();
        g.console = console;
        g
    }

    /// A UMD: its shell's half circle at the top and square-ish bottom, no hole, the cap white.
    #[test]
    fn a_umd_and_its_shell() {
        let dir = TempDir::new("umd");
        let psp = dir.path().join("PSP");
        std::fs::create_dir_all(&psp).unwrap();
        let iso = psp.join("Lumen Drift (USA).iso");
        std::fs::write(&iso, b"").unwrap();
        let fonts = Fonts::load();
        let img = disc(&game(&iso, Console::Psp), &fonts, None, None);
        assert_eq!((img.w, img.h), SIZE);
        let px = |x: usize, y: usize| &img.px[(y * img.w + x) * 4..(y * img.w + x) * 4 + 4];
        // The top corners are outside the half circle; the bottom's arc is lower in the middle
        // than at the sides.
        assert_eq!(px(12, 12)[3], 0);
        assert_eq!(px(img.w - 12, 12)[3], 0);
        assert_eq!(px(img.w / 2, img.h - 7)[3], 255);
        assert_eq!(px(40, img.h - 7)[3], 0);
        assert_eq!(px(img.w - 40, img.h - 7)[3], 0);
        // The sides are straight down to the arc's corners, 10 mm above the bottom.
        assert_eq!(px(30, img.h - 90)[3], 255);
        assert_eq!(px(30, img.h - 40)[3], 0);
        // No hole: the hub is a white cap, solid.
        let (cx, cy) = (img.w / 2, 4 + (32.0 * (img.h as f32 - 8.0) / 65.0) as usize);
        let cap = px(cx, cy - 55);
        assert_eq!(cap[3], 255);
        assert!(cap[..3].iter().all(|&v| v > 230), "{cap:?}");
    }

    #[test]
    fn a_disc_and_its_hole() {
        let dir = TempDir::new("disc");
        let ps = dir.path().join("PS");
        std::fs::create_dir_all(&ps).unwrap();
        let one = ps.join("Racer (USA).chd");
        std::fs::write(&one, b"").unwrap();
        let fonts = Fonts::load();
        let img = disc(&game(&one, Console::PlayStation), &fonts, None, None);
        let a = |x: usize, y: usize| img.px[(y * img.w + x) * 4 + 3];
        // The hole and the corners are clear; the face is solid.
        assert_eq!(a(SIZE.0 / 2, SIZE.1 / 2), 0);
        assert_eq!(a(2, 2), 0);
        assert_eq!(a(SIZE.0 / 2, SIZE.1 / 2 + 160), 255);
        // Two discs listed: the badge, gold, low on the disc.
        let m3u = ps.join("Epic (USA).m3u");
        std::fs::write(&m3u, "# discs\nEpic (Disc 1).chd\nEpic (Disc 2).chd\n").unwrap();
        let two = game(&m3u, Console::PlayStation);
        assert_eq!(discs(&two), 2);
        let img = disc(&two, &fonts, None, None);
        let row = (SIZE.1 as f32 * 0.78 + 4.0) as usize * img.w * 4;
        let gold = img.px[row..row + img.w * 4]
            .chunks(4)
            .any(|p| p[..3] == [255, 196, 60]);
        assert!(gold);
    }
}
