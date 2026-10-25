//! A cart's plastic in a colour and finish of the player's choosing (`cart_shell.txt`, studio.rs),
//! made from the system's own shell. The shell's relief is kept: each pixel's brightness against
//! the plastic's own is laid over the new colour, so the ribs, the well and the lit and shaded
//! edges stay where they were. Then the finish:
//!
//! - solid: the colour, opaque;
//! - clear: see-through, the board behind it showing faintly (its chips and the gold contacts),
//!   the shelf showing where there is no board, and the edges catching the light;
//! - glitter: the colour flecked through with metal flake, and sparkles here and there.
//!
//! It works on any system's shell; only the board's place behind a clear one is the system's.

use super::studio::Finish;
use super::{gb, gba, gbc, Frame, GBA_CART_H, GBA_CART_W, GB_CART_H, GB_CART_W};
use crate::gfx::Img;
use crate::rom::Console;

/// What is behind the plastic, in the cart's pixels: the board, its chips, and its contacts
/// along the bottom edge, with how many pins.
struct Board {
    board: (f32, f32, f32, f32),
    chips: [(f32, f32, f32, f32); 2],
    contacts: (f32, f32, f32, f32),
    pins: usize,
}

/// The board inside a `console` cart drawn `w` x `h`: measured for the Game Boy (the clear GBC
/// cart's) and the GBA, and for the others a board filling most of the cart.
fn board(console: Console, w: usize, h: usize) -> Board {
    let scale = |r: (f32, f32, f32, f32), (gw, gh): (f32, f32), (w, h): (usize, usize)| {
        let (fx, fy) = (w as f32 / gw, h as f32 / gh);
        (r.0 * fx, r.1 * fy, r.2 * fx, r.3 * fy)
    };
    match console {
        Console::Gb | Console::Gbc => {
            let size = (GB_CART_W, GB_CART_H);
            Board {
                board: scale(gbc::BOARD, gb::GRID, size),
                chips: gbc::CHIPS.map(|c| scale(c, gb::GRID, size)),
                contacts: scale(gbc::CONTACTS, gb::GRID, size),
                pins: gbc::PINS,
            }
        }
        // The GBA's board (AGB-E05): the width of the body below the grip, two chips on it,
        // and the contacts along the bottom edge, where the cart goes into the slot.
        Console::Gba => {
            let size = (GBA_CART_W, GBA_CART_H);
            Board {
                board: scale((110.0, 170.0, 1600.0, 860.0), gba::GRID, size),
                chips: [
                    scale((330.0, 300.0, 420.0, 250.0), gba::GRID, size),
                    scale((900.0, 330.0, 260.0, 180.0), gba::GRID, size),
                ],
                contacts: scale((250.0, 990.0, 1320.0, 52.0), gba::GRID, size),
                pins: 32,
            }
        }
        _ => {
            let f = |x: f32, y: f32, fw: f32, fh: f32| {
                (x * w as f32, y * h as f32, fw * w as f32, fh * h as f32)
            };
            Board {
                board: f(0.08, 0.05, 0.84, 0.9),
                chips: [f(0.2, 0.12, 0.22, 0.11), f(0.54, 0.15, 0.15, 0.08)],
                contacts: f(0.1, 0.93, 0.8, 0.045),
                pins: 30,
            }
        }
    }
}

const BOARD_COLOUR: [f32; 3] = [34.0, 92.0, 58.0];
const CHIP_COLOUR: [f32; 3] = [22.0, 22.0, 26.0];
const GOLD: [f32; 3] = [196.0, 158.0, 72.0];

/// A hash of a pixel and a seed, 0 to 1.
fn noise(x: usize, y: usize, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((y as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add(seed.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// `base` in `colour` (or its own colour) and `finish`, for a `console` cart.
pub fn restyle(base: &Frame, console: Console, colour: Option<[u8; 3]>, finish: Finish) -> Frame {
    let (w, h) = (base.img.w, base.img.h);
    let px = &base.img.px;
    let lum = |i: usize| (px[i] as f32 + px[i + 1] as f32 + px[i + 2] as f32) / 3.0;

    // The plastic's own brightness: the median of its opaque pixels, so that the relief is
    // read against the face, whatever the shell's colour.
    let mut opaque: Vec<u8> = (0..w * h)
        .filter(|p| px[p * 4 + 3] == 255)
        .map(|p| lum(p * 4) as u8)
        .collect();
    let face = if opaque.is_empty() {
        128.0
    } else {
        let mid = opaque.len() / 2;
        *opaque.select_nth_unstable(mid).1 as f32
    }
    .max(1.0);

    let behind = board(console, w, h);
    let inside = behind.inside();
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    // Near the outline, or the sticker's window: where a clear shell's edge catches the light.
    let edge = |x: usize, y: usize| {
        let near = |dx: isize, dy: isize| {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            nx < 0
                || ny < 0
                || nx >= w as isize
                || ny >= h as isize
                || px[(ny as usize * w + nx as usize) * 4 + 3] < 128
        };
        let d = 2;
        [(-d, 0), (d, 0), (0, -d), (0, d)]
            .iter()
            .any(|&(dx, dy)| near(dx, dy))
    };

    let mut img = Img {
        w,
        h,
        px: px.clone(),
    };
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let alpha = px[i + 3] as f32 / 255.0;
            if alpha == 0.0 {
                continue;
            }
            // How much lighter or darker than the face this pixel is.
            let s = lum(i) / face;
            let own = [px[i] as f32, px[i + 1] as f32, px[i + 2] as f32];
            let tint = colour.map_or(own.map(|v| v / s.max(0.05)), |c| c.map(f32::from));
            let shade = |t: f32| {
                if s <= 1.0 {
                    t * s
                } else {
                    t + (255.0 - t) * ((s - 1.0) * 1.2).min(1.0)
                }
            };
            let mut rgb = tint.map(shade);
            let mut a = alpha;
            match finish {
                Finish::Solid => {}
                Finish::Clear => {
                    let (on, behind) = inside(x as f32 + 0.5, y as f32 + 0.5);
                    let shine =
                        ((s - 1.0) * 1.5).clamp(0.0, 0.3) + if edge(x, y) { 0.22 } else { 0.0 };
                    let plastic = rgb.map(|v| mix(v, 255.0, shine));
                    // The board is seen through the plastic, so in its colour, and darker.
                    let filter = tint.map(|t| 0.25 + 0.75 * t / 255.0);
                    let seen: [f32; 3] =
                        std::array::from_fn(|k| behind[k] * filter[k] * s.min(1.2));
                    rgb = std::array::from_fn(|k| mix(plastic[k], seen[k], 0.58 * on));
                    // Over nothing the shelf shows through, less so where the light catches it.
                    a = alpha * mix(0.55 + shine, 0.94, on).min(1.0);
                }
                Finish::Glitter => {
                    // Flake: every pixel a touch lighter or darker, in clusters of two.
                    let flake = noise(x / 2, y / 2, 1) - 0.5;
                    rgb = rgb.map(|v| v * (1.0 + flake * 0.22));
                    // Sparkles: one pixel in sixty lit, the brightest with a cross of light.
                    let lit = |x: usize, y: usize| {
                        let v = noise(x, y, 2);
                        (v < 1.0 / 60.0).then(|| noise(x, y, 3))
                    };
                    let mut spark = lit(x, y).map_or(0.0, |v| 0.45 + 0.55 * v);
                    for (dx, dy) in [(1, 0), (0, 1), (2, 0), (0, 2)] {
                        let far = if dx + dy == 2 { 0.45 } else { 0.25 };
                        for (nx, ny) in [(x + dx, y + dy), (x.wrapping_sub(dx), y.wrapping_sub(dy))]
                        {
                            if let Some(v) = lit(nx, ny).filter(|v| *v > 0.8) {
                                spark = spark.max(v * far);
                            }
                        }
                    }
                    // Some sparkles white, some in a lighter shade of the plastic's colour.
                    let to = if noise(x, y, 4) < 0.5 {
                        [255.0; 3]
                    } else {
                        tint.map(|t| t * 0.5 + 128.0)
                    };
                    rgb = std::array::from_fn(|k| mix(rgb[k], to[k], spark));
                }
            }
            for (k, v) in rgb.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (a * 255.0).round() as u8;
        }
    }
    Frame {
        img,
        sticker: base.sticker,
    }
}

impl Board {
    /// How much of a point is on the board, and the colour there.
    fn inside(&self) -> impl Fn(f32, f32) -> (f32, [f32; 3]) + '_ {
        move |x, y| {
            let within = |(rx, ry, rw, rh): (f32, f32, f32, f32), soft: f32| {
                let d = (rx - x).max(x - rx - rw).max(ry - y).max(y - ry - rh);
                (0.5 - d / soft).clamp(0.0, 1.0)
            };
            let on = within(self.board, 2.0);
            let mut c = BOARD_COLOUR;
            for chip in self.chips {
                let t = within(chip, 1.0);
                c = std::array::from_fn(|k| c[k] + (CHIP_COLOUR[k] - c[k]) * t);
            }
            let (cx, _, cw, _) = self.contacts;
            let pitch = cw / self.pins as f32;
            let pin = x > cx && ((x - cx) / pitch).fract() < 0.62;
            let gold = within(self.contacts, 1.0) * f32::from(u8::from(pin));
            c = std::array::from_fn(|k| c[k] + (GOLD[k] - c[k]) * gold);
            (on, c)
        }
    }
}
