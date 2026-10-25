//! Cart shells drawn after real carts, measured from photos of their fronts: the Mega Drive's,
//! the Game Gear's, and the SNES's two shapes, the US one and the Super Famicom's (Japan, Europe
//! and everywhere else). Each part is placed by its fraction of the cart's width and height, as
//! measured, so the numbers here can be checked against a cart; and each has the label DSlot
//! makes for it when it has no `Labels/` scan: the box art laid out as that system's labels lay
//! it out, with no logos or trademarks.

use super::{cover_label, printed_gba, rounded, Frame, Game, Img};
use crate::gfx::Fonts;

/// An RGB colour, worked in floating point and clamped when it is stored.
type Rgb = [f32; 3];

/// A shell `w` by `h` pixels: `at(x, y)` gives each pixel's colour and how far inside the
/// cart's outline it is (an SDF: negative inside), `window` how far inside the label's window.
fn paint(
    (w, h): (usize, usize),
    at: impl Fn(f32, f32) -> (Rgb, f32),
    window: (f32, f32, f32, f32),
    radius: f32,
) -> Frame {
    let mut img = Img::new(w, h);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let (c, body) = at(x, y);
            let win = rounded(x, y, window, [radius; 4]);
            let i = (py * w + px) * 4;
            for (k, v) in c.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] =
                ((0.5 - body).clamp(0.0, 1.0) * (0.5 + win).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    let (x, y, sw, sh) = window;
    Frame {
        img,
        sticker: (
            x as i32 - 1,
            y as i32 - 1,
            sw.ceil() as i32 + 2,
            sh.ceil() as i32 + 2,
        ),
    }
}

/// A rectangle from fractions of `size`: (left, top, right, bottom).
fn rect((w, h): (f32, f32), (l, t, r, b): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    (l * w, t * h, (r - l) * w, (b - t) * h)
}

/// `c` scaled, as light or shadow falls on it.
fn lit(c: Rgb, k: f32) -> Rgb {
    c.map(|v| v * k)
}

/// A line `half` pixels either side of `d = 0`, fading out: 1 on it, 0 off it.
fn line(d: f32, half: f32) -> f32 {
    (1.0 - d.abs() / half).clamp(0.0, 1.0)
}

/// How far inside an ellipse centred at `c` with radii `r` a point is, roughly in pixels.
fn ellipse(x: f32, y: f32, c: (f32, f32), r: (f32, f32)) -> f32 {
    let (dx, dy) = ((x - c.0) / r.0, (y - c.1) / r.1);
    ((dx * dx + dy * dy).sqrt() - 1.0) * r.0.min(r.1)
}

/// A label's art: the box art filling `w` by `h`, or a printed one without it.
fn art(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    match cover {
        Some(_) => cover_label(cover, w, h),
        None => printed_gba(game, fonts, w, h, None),
    }
}

/// `src` drawn into `img` at `(l, t, r, b)`, fractions of `img`.
fn place(img: &mut Img, src: &Img, (l, t, r, b): (f32, f32, f32, f32)) {
    let (w, h) = (img.w as f32, img.h as f32);
    let dst = (
        (l * w) as i32,
        (t * h) as i32,
        ((r - l) * w) as i32,
        ((b - t) * h) as i32,
    );
    img.blit(src, (0.0, 0.0, src.w as f32, src.h as f32), dst, false);
}

/// The size, in pixels, of the art inside a label `w` by `h` at `(l, t, r, b)`.
fn inner(w: i32, h: i32, (l, t, r, b): (f32, f32, f32, f32)) -> (i32, i32) {
    (((r - l) * w as f32) as i32, ((b - t) * h as f32) as i32)
}

pub mod md {
    //! A Genesis cart (Sonic 2), front on: 385 x 250 in the photo.
    pub const SIZE: (usize, usize) = (640, 416);
    /// The raised middle, which reaches the top edge; the sides start a little lower.
    pub const MIDDLE: (f32, f32) = (0.14, 0.86);
    pub const SIDES_TOP: f32 = 0.016;
    /// How much narrower the cart is at the bottom than the top, each side.
    pub const TAPER: f32 = 0.010;
    pub const WINDOW: (f32, f32, f32, f32) = (0.151, 0.02, 0.847, 0.96);
    /// On the label: the art at the top, the grid band under it.
    pub const ART: (f32, f32, f32, f32) = (0.0, 0.0, 1.0, 0.68);
    pub const PLASTIC: super::Rgb = [40.0, 40.0, 42.0];
}

/// The Mega Drive's black cart: wide, the raised middle up to the top edge with the sides a
/// step lower, the sides tapering a little toward the bottom, and a recessed label window over
/// most of the face.
pub fn md_frame() -> Frame {
    use md::*;
    let (fw, fh) = (SIZE.0 as f32, SIZE.1 as f32);
    let window = rect((fw, fh), WINDOW);
    let middle = rect((fw, fh), (MIDDLE.0, 0.0, MIDDLE.1, 1.0));
    paint(
        SIZE,
        |x, y| {
            let ty = y / fh;
            let taper = TAPER * fw * ty;
            let sides = rounded(
                x,
                y,
                (
                    taper,
                    SIDES_TOP * fh,
                    fw - 2.0 * taper,
                    fh * (1.0 - SIDES_TOP),
                ),
                [0.03 * fw, 0.03 * fw, 0.055 * fw, 0.055 * fw],
            );
            let mid = rounded(x, y, middle, [0.012 * fw, 0.012 * fw, 0.0, 0.0]);
            let body = sides.min(mid);
            let in_middle = mid < 0.0;
            // Lighter toward the bottom, as the photo's light falls; the middle stands proud.
            let mut c = lit(PLASTIC, 0.9 + 0.35 * ty);
            if in_middle {
                c = lit(c, 1.12);
            }
            // The steps from the middle down to the sides, a shadow line each.
            let step = line(mid, 1.5) * (ty > SIDES_TOP + 0.01) as u8 as f32;
            c = lit(c, 1.0 - 0.45 * step);
            // The window's recess: a dark rim round it, a highlight on its lower lip.
            let win = rounded(x, y, window, [0.01 * fw; 4]);
            c = lit(c, 1.0 - 0.5 * line(win - 2.0, 2.5));
            c = lit(
                c,
                1.0 + 0.6 * line(win - 4.5, 1.2) * (y > window.1 + window.3 - 2.0) as u8 as f32,
            );
            // Darker toward the outline, so the plastic reads as rounded.
            c = lit(c, 1.0 - (body + 5.0).clamp(0.0, 5.0) * 0.06);
            (c, body)
        },
        window,
        0.01 * fw,
    )
}

/// A Mega Drive label: the art at the top, a black band ruled in a grey grid along the bottom
/// (where the system's own logo goes on a real one).
pub fn md_label(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    let band = (md::ART.3 * h as f32) as usize;
    let cell = (w as f32 / 24.0).max(4.0);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let (fx, fy) = (x as f32 % cell, (y.saturating_sub(band)) as f32 % cell);
            let rule = y >= band && (fx < 1.2 || fy < 1.2 || y < band + 3);
            let v = if rule { 96 } else { 14 };
            img.px[(y * w as usize + x) * 4..][..4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let (aw, ah) = inner(w, h, md::ART);
    place(&mut img, &art(game, fonts, aw, ah, cover), md::ART);
    img
}

pub mod gg {
    //! A Game Gear cart (Urban Strike), front on: 650 x 638, its tilt in the photo evened out.
    pub const SIZE: (usize, usize) = (500, 490);
    /// The raised band across the top, whose sides run down beside the label to here.
    pub const BAND: f32 = 0.37;
    /// The band's inner edge, round the top of the label.
    pub const HOOD: (f32, f32, f32, f32) = (0.065, 0.15, 0.935, 0.95);
    /// The two fine ridges at the middle of the band.
    pub const RIDGES: [f32; 2] = [0.085, 0.105];
    pub const RIDGE_X: (f32, f32) = (0.24, 0.76);
    /// Below the band the cart is this much narrower, each side.
    pub const STEP: f32 = 0.011;
    pub const WINDOW: (f32, f32, f32, f32) = (0.08, 0.165, 0.90, 0.765);
    /// The oval badge at the bottom: centre and radii.
    pub const BADGE: (f32, f32, f32, f32) = (0.49, 0.885, 0.20, 0.055);
    pub const PLASTIC: super::Rgb = [92.0, 93.0, 98.0];
    /// On the label: the coloured strip down its left edge, and its colour.
    pub const STRIP: f32 = 0.13;
    pub const MAGENTA: [u8; 3] = [222, 64, 178];
}

/// The Game Gear's charcoal cart: close to square, rounded, a raised band across the top whose
/// sides come down beside the label and step in where they meet the body, two fine ridges at
/// the band's middle, the label window in the middle, and a blank oval badge below it.
pub fn gg_frame() -> Frame {
    use gg::*;
    let (fw, fh) = (SIZE.0 as f32, SIZE.1 as f32);
    let window = rect((fw, fh), WINDOW);
    let hood = rect((fw, fh), HOOD);
    paint(
        SIZE,
        |x, y| {
            let (tx, ty) = (x / fw, y / fh);
            let band = rounded(
                x,
                y,
                (0.0, 0.0, fw, BAND * fh),
                [0.06 * fw, 0.06 * fw, 0.01 * fw, 0.01 * fw],
            );
            let below = rounded(
                x,
                y,
                (STEP * fw, 0.3 * fh, fw * (1.0 - 2.0 * STEP), 0.7 * fh),
                [0.0, 0.0, 0.05 * fw, 0.05 * fw],
            );
            let body = band.min(below);
            // On the band (its top and its sides beside the label), or the body below it.
            let inner = rounded(x, y, hood, [0.07 * fw; 4]);
            let on_band = band < 0.0 && inner > 0.0;
            let mut c = if on_band {
                // The band curves away at its edges: lighter down its sides.
                lit(
                    PLASTIC,
                    0.85 + 0.35 * (ty / BAND) * ((tx - 0.5).abs() * 2.0),
                )
            } else {
                lit(PLASTIC, 1.0 + 0.1 * ty)
            };
            // The band's inner edge, and where it steps down to the body: shadow lines.
            c = lit(
                c,
                1.0 - 0.45 * line(inner, 1.6) * (y < BAND * fh) as u8 as f32,
            );
            c = lit(
                c,
                1.0 - 0.4 * line(y - BAND * fh, 1.5) * (inner > 0.0) as u8 as f32,
            );
            // The two ridges: a dark groove with a highlight under it.
            if (RIDGE_X.0..RIDGE_X.1).contains(&tx) {
                for r in RIDGES {
                    c = lit(
                        c,
                        1.0 - 0.4 * line(y - r * fh, 1.2) + 0.35 * line(y - r * fh - 2.0, 1.0),
                    );
                }
            }
            // The label window's recess.
            let win = rounded(x, y, window, [0.025 * fw; 4]);
            c = lit(c, 1.0 - 0.5 * line(win - 2.0, 2.5));
            // The badge: an embossed oval, its rim catching the light at the top.
            let (bx, by, rx, ry) = BADGE;
            let badge = ellipse(x, y, (bx * fw, by * fh), (rx * fw, ry * fh));
            c = lit(c, 1.0 - 0.35 * line(badge, 2.0));
            if badge < -2.0 {
                c = lit(c, 1.06);
            }
            c = lit(
                c,
                1.0 + 0.3 * line(badge + 3.0, 1.0) * (y < by * fh) as u8 as f32,
            );
            c = lit(c, 1.0 - (body + 5.0).clamp(0.0, 5.0) * 0.06);
            (c, body)
        },
        window,
        0.025 * fw,
    )
}

/// A Game Gear label: a coloured strip down its left edge (where the system's name runs on a
/// real one), the art beside it.
pub fn gg_label(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    let [r, g, b] = gg::MAGENTA;
    img.rrect(0.0, 0.0, w as f32, h as f32, [4.0; 4], [r, g, b, 255]);
    let area = (gg::STRIP, 0.0, 1.0, 1.0);
    let (aw, ah) = inner(w, h, area);
    place(&mut img, &art(game, fonts, aw, ah, cover), area);
    img
}

/// The SNES cart's canvas: both shapes are drawn on it, the Super Famicom's a little narrower.
pub const SNES_SIZE: (usize, usize) = (640, 410);

pub mod us {
    //! A US SNES cart (Super Mario World), front on: 1020 x 653 in the photo.
    /// The raised middle carrying the label; the stepped wings either side start a little lower.
    pub const MIDDLE: (f32, f32) = (0.158, 0.842);
    pub const WINGS_TOP: f32 = 0.018;
    /// The grooves across each wing.
    pub const GROOVES: [f32; 5] = [0.184, 0.348, 0.510, 0.674, 0.838];
    pub const WINDOW: (f32, f32, f32, f32) = (0.189, 0.005, 0.814, 0.447);
    /// The grip below the label, in three parts (the middle one deeper).
    pub const GRIP: (f32, f32, f32, f32) = (0.186, 0.593, 0.804, 0.669);
    pub const GRIP_MIDDLE: (f32, f32) = (0.40, 0.587);
    /// The screws in the bottom corners: centres, and their radius as a fraction of the width.
    pub const SCREWS: [(f32, f32); 2] = [(0.087, 0.922), (0.911, 0.922)];
    pub const SCREW: f32 = 0.016;
    pub const PLASTIC: super::Rgb = [146.0, 141.0, 151.0];
    /// On the label: the art, between black side panels, and the red strip at the top right.
    pub const ART: (f32, f32, f32, f32) = (0.152, 0.03, 0.697, 0.97);
    pub const RED: (f32, f32, f32, f32) = (0.714, 0.0, 0.99, 0.05);
}

pub mod famicom {
    //! A Super Famicom style cart (the PAL and Japanese shape), front on: 1018 x 692 in the photo.
    /// Its width on the SNES canvas: it is a little less wide for its height than the US one.
    pub const WIDTH: f32 = 0.946;
    /// The top: curved a little, and its corners well rounded.
    pub const ARCH: f32 = 0.008;
    pub const CORNER: f32 = 0.06;
    /// The short slots on each upper side: across, and down (the first, the step, the height).
    pub const SLOTS_X: (f32, f32) = (0.013, 0.044);
    pub const SLOTS_Y: (f32, f32, f32) = (0.087, 0.038, 0.014);
    pub const WINDOW: (f32, f32, f32, f32) = (0.088, 0.055, 0.904, 0.468);
    /// Where the lower sides step out from the middle.
    pub const SIDES: (f32, f32) = (0.154, 0.835);
    pub const GRIP: (f32, f32, f32, f32) = (0.20, 0.616, 0.79, 0.70);
    pub const GRIP_MIDDLE: (f32, f32) = (0.388, 0.599);
    pub const SCREWS: [(f32, f32); 2] = [(0.057, 0.916), (0.933, 0.922)];
    pub const SCREW: f32 = 0.018;
    pub const PLASTIC: super::Rgb = [166.0, 172.0, 174.0];
    /// On the label: the art boxed in the middle-right of a white label, and four coloured dots
    /// at the bottom left, as the system's own button colours (no name, no logo).
    pub const ART: (f32, f32, f32, f32) = (0.34, 0.07, 0.83, 0.92);
    pub const DOTS: (f32, f32) = (0.13, 0.78);
}

/// The grip below a SNES label: a long rounded recess in three parts, the middle one deeper.
fn grip(
    c: Rgb,
    x: f32,
    y: f32,
    (fw, fh): (f32, f32),
    at: (f32, f32, f32, f32),
    middle: (f32, f32),
) -> Rgb {
    let r = rect((fw, fh), at);
    let d = rounded(x, y, r, [r.3 / 2.0; 4]);
    if d > 1.5 {
        return c;
    }
    let tx = x / fw;
    let deep = (middle.0..middle.1).contains(&tx);
    let mut c = lit(c, if deep { 0.72 } else { 0.84 });
    // Its walls: shadow at the top, light on the lower lip, and the dividers.
    c = lit(c, 1.0 - 0.35 * line(y - r.1, 2.5));
    c = lit(c, 1.0 + 0.3 * line(y - (r.1 + r.3), 2.0));
    c = lit(
        c,
        1.0 - 0.4 * (line(tx * fw - middle.0 * fw, 1.5) + line(tx * fw - middle.1 * fw, 1.5)),
    );
    lit(c, 1.0 - 0.4 * line(d, 1.5))
}

/// The screws in a SNES cart's bottom corners.
fn screws(
    c: Rgb,
    x: f32,
    y: f32,
    (fw, fh): (f32, f32),
    at: [(f32, f32); 2],
    radius: f32,
    metal: Rgb,
) -> Rgb {
    let mut c = c;
    for (sx, sy) in at {
        let d = ellipse(x, y, (sx * fw, sy * fh), (radius * fw, radius * fw));
        if d < 0.0 {
            // The head, lit from above, and its rim.
            let k = 1.1 - 0.4 * ((y - sy * fh) / (radius * fw)).clamp(-1.0, 1.0);
            c = lit(metal, k * (1.0 - 0.5 * line(d + 1.0, 1.5)));
        } else {
            c = lit(c, 1.0 - 0.4 * line(d, 1.5));
        }
    }
    c
}

/// The US SNES cart: purple-grey, wide and squared off; the raised middle carries the label,
/// the stepped wings either side have grooves across them; a grip below the label, a screw in
/// each bottom corner.
pub fn snes_us_frame() -> Frame {
    use us::*;
    let (fw, fh) = (SNES_SIZE.0 as f32, SNES_SIZE.1 as f32);
    let window = rect((fw, fh), WINDOW);
    let middle = rect((fw, fh), (MIDDLE.0, 0.0, MIDDLE.1, 1.0));
    paint(
        SNES_SIZE,
        |x, y| {
            let (tx, ty) = (x / fw, y / fh);
            let wings = rounded(
                x,
                y,
                (0.0, WINGS_TOP * fh, fw, (1.0 - WINGS_TOP) * fh),
                [0.012 * fw; 4],
            );
            let mid = rounded(x, y, middle, [0.008 * fw, 0.008 * fw, 0.0, 0.0]);
            let body = wings.min(mid);
            let on_wing = mid > 0.0;
            let mut c = lit(
                PLASTIC,
                if on_wing { 1.05 } else { 0.97 } + 0.04 * (1.0 - ty),
            );
            if on_wing {
                for g in GROOVES {
                    c = lit(
                        c,
                        1.0 - 0.4 * line(y - g * fh, 1.4) + 0.25 * line(y - g * fh - 2.2, 1.0),
                    );
                }
            }
            // The steps between the middle and the wings.
            c = lit(
                c,
                1.0 - 0.4 * line(mid, 1.5) * (ty > WINGS_TOP + 0.01) as u8 as f32,
            );
            let win = rounded(x, y, window, [0.008 * fw; 4]);
            c = lit(c, 1.0 - 0.45 * line(win - 2.0, 2.0));
            c = grip(c, x, y, (fw, fh), GRIP, GRIP_MIDDLE);
            c = screws(c, x, y, (fw, fh), SCREWS, SCREW, [175.0, 175.0, 178.0]);
            c = lit(
                c,
                1.0 - (body + 5.0).clamp(0.0, 5.0) * 0.05 - 0.02 * (tx - 0.5).abs(),
            );
            (c, body)
        },
        window,
        0.008 * fw,
    )
}

/// A US SNES label: black, the art in the middle between the side panels, a red strip at the
/// top right.
pub fn us_label(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    img.rrect(0.0, 0.0, w as f32, h as f32, [3.0; 4], [16, 16, 18, 255]);
    let (l, t, r, b) = us::RED;
    img.rrect(
        l * w as f32,
        t * h as f32,
        (r - l) * w as f32,
        (b - t) * h as f32,
        [1.0; 4],
        [196, 22, 40, 255],
    );
    let (aw, ah) = inner(w, h, us::ART);
    place(&mut img, &art(game, fonts, aw, ah, cover), us::ART);
    img
}

/// The Super Famicom style cart: light grey, its top curved a little with well-rounded corners,
/// short slots on each upper side, one wide label across the upper body, the lower sides
/// stepped out, the grip below, a screw in each bottom corner.
pub fn snes_famicom_frame() -> Frame {
    use famicom::*;
    let (cw, fh) = (SNES_SIZE.0 as f32, SNES_SIZE.1 as f32);
    let fw = cw * WIDTH;
    let x0 = (cw - fw) / 2.0;
    let window = {
        let (x, y, w, h) = rect((fw, fh), WINDOW);
        (x + x0, y, w, h)
    };
    paint(
        SNES_SIZE,
        |x, y| {
            let lx = x - x0;
            let (tx, ty) = (lx / fw, y / fh);
            // The top arches up a little to the middle.
            let arch = ARCH * fh * (1.0 - (2.0 * tx - 1.0).powi(2)).max(0.0);
            let body = rounded(
                lx,
                y + arch,
                (0.0, ARCH * fh, fw, fh * (1.0 - ARCH)),
                [CORNER * fw, CORNER * fw, 0.02 * fw, 0.02 * fw],
            );
            let mut c = lit(PLASTIC, 1.0 + 0.05 * (1.0 - ty));
            // The slots on each upper side.
            let (first, step, height) = SLOTS_Y;
            let side = tx < 0.5;
            let sx = if side { tx } else { 1.0 - tx };
            if (SLOTS_X.0..SLOTS_X.1).contains(&sx) {
                for i in 0..6 {
                    let top = (first + i as f32 * step) * fh;
                    if (top..top + height * fh).contains(&y) {
                        c = lit(c, 0.62 + 0.25 * ((y - top) / (height * fh)));
                    }
                }
            }
            // The lower sides, stepped out from the middle below the label.
            if ty > WINDOW.3 + 0.005 {
                c = lit(
                    c,
                    1.0 - 0.35 * (line(lx - SIDES.0 * fw, 1.3) + line(lx - SIDES.1 * fw, 1.3)),
                );
            }
            let win = rounded(x, y, window, [0.012 * fw; 4]);
            c = lit(c, 1.0 - 0.4 * line(win - 2.0, 2.0));
            c = grip(c, lx, y, (fw, fh), GRIP, GRIP_MIDDLE);
            c = screws(c, lx, y, (fw, fh), SCREWS, SCREW, [184.0, 156.0, 92.0]);
            c = lit(c, 1.0 - (body + 5.0).clamp(0.0, 5.0) * 0.05);
            (c, body)
        },
        window,
        0.012 * fw,
    )
}

/// A Super Famicom style label: white, the art boxed in black toward the right, and the four
/// button colours as dots at the bottom left.
pub fn famicom_label(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    img.rrect(0.0, 0.0, w as f32, h as f32, [3.0; 4], [244, 244, 240, 255]);
    let (l, t, r, b) = famicom::ART;
    let (fw, fh) = (w as f32, h as f32);
    img.rrect(
        l * fw - 2.0,
        t * fh - 2.0,
        (r - l) * fw + 4.0,
        (b - t) * fh + 4.0,
        [1.0; 4],
        [20, 20, 22, 255],
    );
    let (aw, ah) = inner(w, h, famicom::ART);
    place(&mut img, &art(game, fonts, aw, ah, cover), famicom::ART);
    // Red right, yellow below, green left, blue above, overlapping a little.
    let (cx, cy) = (famicom::DOTS.0 * fw, famicom::DOTS.1 * fh);
    let d = 0.035 * fw;
    let dots = [
        ((cx, cy - d * 0.8), [52, 72, 176]),
        ((cx - d, cy), [40, 160, 82]),
        ((cx + d, cy), [210, 40, 52]),
        ((cx, cy + d * 0.8), [236, 196, 36]),
    ];
    for ((dx, dy), [r, g, b]) in dots {
        let (rx, ry) = (d * 0.75, d * 0.5);
        img.rrect(
            dx - rx,
            dy - ry,
            2.0 * rx,
            2.0 * ry,
            [ry; 4],
            [r, g, b, 255],
        );
    }
    img
}
