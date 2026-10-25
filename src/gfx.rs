//! Software drawing: carts, boxes and text are composed here once, then uploaded as textures.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub type Rgba = [u8; 4];

#[derive(Clone)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Img {
    pub fn new(w: usize, h: usize) -> Img {
        Img {
            w,
            h,
            px: vec![0; w * h * 4],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgba {
        let i = (y * self.w + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    /// Source-over blend of `c` at coverage `cov` (0..1).
    pub fn over(&mut self, x: i32, y: i32, c: Rgba, cov: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let sa = c[3] as f32 / 255.0 * cov;
        if sa <= 0.0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let da = self.px[i + 3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        for (dst, &src) in self.px[i..i + 3].iter_mut().zip(&c[..3]) {
            let v = (src as f32 * sa + *dst as f32 * da * (1.0 - sa)) / oa;
            *dst = v.round() as u8;
        }
        self.px[i + 3] = (oa * 255.0).round() as u8;
    }

    /// Scaled down to `w` x `h` by averaging the pixels under each new one, which keeps a small
    /// cover smooth where picking single pixels would not.
    pub fn shrink(&self, w: usize, h: usize) -> Img {
        let mut out = Img::new(w, h);
        for y in 0..h {
            let y0 = y * self.h / h;
            let y1 = ((y + 1) * self.h / h).clamp(y0 + 1, self.h.max(y0 + 1));
            for x in 0..w {
                let x0 = x * self.w / w;
                let x1 = ((x + 1) * self.w / w).clamp(x0 + 1, self.w.max(x0 + 1));
                // Colour is weighted by alpha, so clear pixels at an edge do not darken it.
                let (mut sum, mut alpha) = ([0u32; 3], 0u32);
                for sy in y0..y1.min(self.h) {
                    for sx in x0..x1.min(self.w) {
                        let p = self.get(sx, sy);
                        for k in 0..3 {
                            sum[k] += p[k] as u32 * p[3] as u32;
                        }
                        alpha += p[3] as u32;
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as u32;
                let px = &mut out.px[(y * w + x) * 4..][..4];
                for k in 0..3 {
                    px[k] = sum[k].checked_div(alpha).unwrap_or(0) as u8;
                }
                px[3] = (alpha / n) as u8;
            }
        }
        out
    }

    /// Anti-aliased rounded rectangle. Radii are top-left, top-right, bottom-right, bottom-left.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: [f32; 4], c: Rgba) {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        for py in y.floor() as i32..(y + h).ceil() as i32 {
            for px in x.floor() as i32..(x + w).ceil() as i32 {
                let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let rad = match (dx < 0.0, dy < 0.0) {
                    (true, true) => r[0],
                    (false, true) => r[1],
                    (false, false) => r[2],
                    (true, false) => r[3],
                };
                let (qx, qy) = (dx.abs() - w / 2.0 + rad, dy.abs() - h / 2.0 + rad);
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad;
                self.over(px, py, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Draw the `src` sub-rectangle scaled into the destination rectangle.
    /// `nearest` keeps pixel art crisp; otherwise each pixel is a box average of its source area.
    pub fn blit(
        &mut self,
        src: &Img,
        s: (f32, f32, f32, f32),
        d: (i32, i32, i32, i32),
        nearest: bool,
    ) {
        let (sx, sy, sw, sh) = s;
        let (dx, dy, dw, dh) = d;
        if dw <= 0 || dh <= 0 || src.w == 0 || src.h == 0 {
            return;
        }
        let (fx, fy) = (sw / dw as f32, sh / dh as f32);
        for y in 0..dh {
            for x in 0..dw {
                let (x0, y0) = (sx + x as f32 * fx, sy + y as f32 * fy);
                let c = if nearest {
                    let px = ((x0 + fx / 2.0) as usize).min(src.w - 1);
                    let py = ((y0 + fy / 2.0) as usize).min(src.h - 1);
                    src.get(px, py)
                } else {
                    src.average(x0, y0, x0 + fx, y0 + fy)
                };
                self.over(dx + x, dy + y, c, 1.0);
            }
        }
    }

    fn average(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rgba {
        let xa = (x0.max(0.0) as usize).min(self.w - 1);
        let ya = (y0.max(0.0) as usize).min(self.h - 1);
        let xb = (x1.ceil() as usize).clamp(xa + 1, self.w);
        let yb = (y1.ceil() as usize).clamp(ya + 1, self.h);
        let mut acc = [0u32; 4];
        for y in ya..yb {
            for x in xa..xb {
                let p = self.get(x, y);
                let a = p[3] as u32;
                acc[0] += p[0] as u32 * a;
                acc[1] += p[1] as u32 * a;
                acc[2] += p[2] as u32 * a;
                acc[3] += a;
            }
        }
        let n = ((xb - xa) * (yb - ya)) as u32;
        if acc[3] == 0 {
            return [0; 4];
        }
        [
            (acc[0] / acc[3]) as u8,
            (acc[1] / acc[3]) as u8,
            (acc[2] / acc[3]) as u8,
            (acc[3] / n) as u8,
        ]
    }

    /// Tint a white-on-transparent image (rendered text) and draw it.
    pub fn stamp(&mut self, mask: &Img, x: i32, y: i32, c: Rgba) {
        for my in 0..mask.h {
            for mx in 0..mask.w {
                let a = mask.px[(my * mask.w + mx) * 4 + 3];
                if a > 0 {
                    self.over(x + mx as i32, y + my as i32, c, a as f32 / 255.0);
                }
            }
        }
    }
}

/// A shape `size` pixels square in white, to be tinted when drawn. `inside` takes a point
/// in -1..1 on both axes, y down.
fn shape(size: usize, inside: impl Fn(f32, f32) -> bool) -> Img {
    let mut img = Img::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let at =
                |v: usize, k: usize| (v as f32 + (k as f32 + 0.5) / 4.0) / size as f32 * 2.0 - 1.0;
            let hits = (0..16)
                .filter(|k| inside(at(x, k % 4), at(y, k / 4)))
                .count();
            img.px[(y * size + x) * 4..][..4].copy_from_slice(&[
                255,
                255,
                255,
                (hits * 255 / 16) as u8,
            ]);
        }
    }
    img
}

/// Five-pointed star, for ratings.
pub fn star(size: usize) -> Img {
    use std::f32::consts::PI;
    let points: Vec<(f32, f32)> = (0..10)
        .map(|i| {
            let (a, r) = (i as f32 * PI / 5.0, if i % 2 == 0 { 1.0 } else { 0.42 });
            (r * a.sin(), 0.05 - r * a.cos())
        })
        .collect();
    shape(size, |x, y| {
        // Even-odd crossing test against the outline.
        (0..10)
            .filter(|&i| {
                let ((ax, ay), (bx, by)) = (points[i], points[(i + 1) % 10]);
                (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * (bx - ax)
            })
            .count()
            % 2
            == 1
    })
}

/// What the battery icon shows beside its level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    None,
    /// Charging: a bolt.
    Charging,
    /// On its charger, not charging (full, or battery protect holding it): a plug.
    Plugged,
}

/// The status bar's battery: an outline with its terminal, filled to `percent` (red when low
/// and not on a charger), and a bolt or a plug over it, in colour. `h` pixels tall, a little
/// over twice as wide.
pub fn battery(h: usize, percent: u8, mark: Mark) -> Img {
    let w = (h as f32 * 2.1).round() as usize;
    let (fw, fh) = (w as f32 / h as f32, 1.0);
    let rim: Rgba = [200, 204, 212, 255];
    let fill: Rgba = if percent <= 15 && mark == Mark::None {
        [226, 70, 70, 255]
    } else {
        [235, 238, 244, 255]
    };
    let (mark_colour, edge): (Rgba, Rgba) = match mark {
        Mark::Plugged => ([140, 200, 255, 255], [20, 24, 32, 255]),
        _ => ([255, 206, 72, 255], [20, 24, 32, 255]),
    };
    // The body is 1.82 units wide in a 2.1 by 1 box; the terminal sticks out to its right.
    let body = (0.03, 0.06, 1.82, 0.88);
    let level = percent.min(100) as f32 / 100.0;
    let inside_rect = |x: f32, y: f32, r: (f32, f32, f32, f32), rad: f32| {
        let (dx, dy) = (
            (x - r.0 - r.2 / 2.0).abs() - r.2 / 2.0 + rad,
            (y - r.1 - r.3 / 2.0).abs() - r.3 / 2.0 + rad,
        );
        dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - rad < 0.0
    };
    let poly = |x: f32, y: f32, pts: &[(f32, f32)]| {
        (0..pts.len())
            .filter(|&i| {
                let ((ax, ay), (bx, by)) = (pts[i], pts[(i + 1) % pts.len()]);
                (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * (bx - ax)
            })
            .count()
            % 2
            == 1
    };
    let bolt = [
        (1.02, 0.08),
        (0.62, 0.56),
        (0.90, 0.56),
        (0.80, 0.92),
        (1.22, 0.40),
        (0.94, 0.40),
    ];
    let grow = |pts: &[(f32, f32)], k: f32| -> Vec<(f32, f32)> {
        let (cx, cy) = (0.92, 0.5);
        pts.iter()
            .map(|&(x, y)| (cx + (x - cx) * k, cy + (y - cy) * k))
            .collect()
    };
    let plug = |x: f32, y: f32, pad: f32| {
        // A plug's body with two prongs to its right, and its lead to the left.
        inside_rect(
            x,
            y,
            (0.66 - pad, 0.30 - pad, 0.36 + 2.0 * pad, 0.40 + 2.0 * pad),
            0.08,
        ) || inside_rect(
            x,
            y,
            (1.0 - pad, 0.34 - pad, 0.2 + 2.0 * pad, 0.08 + 2.0 * pad),
            0.0,
        ) || inside_rect(
            x,
            y,
            (1.0 - pad, 0.58 - pad, 0.2 + 2.0 * pad, 0.08 + 2.0 * pad),
            0.0,
        ) || inside_rect(
            x,
            y,
            (0.44 - pad, 0.46 - pad, 0.24 + 2.0 * pad, 0.08 + 2.0 * pad),
            0.0,
        )
    };
    let (bolt_edge, bolt_in) = (grow(&bolt, 1.22), bolt.to_vec());
    let colour_at = |x: f32, y: f32| -> Option<Rgba> {
        let on_mark = |pad: f32| match mark {
            Mark::None => false,
            Mark::Charging => poly(x, y, if pad > 0.0 { &bolt_edge } else { &bolt_in }),
            Mark::Plugged => plug(x, y, pad),
        };
        if on_mark(0.0) {
            return Some(mark_colour);
        }
        if on_mark(0.07) {
            return Some(edge);
        }
        let outer = inside_rect(x, y, body, 0.16);
        let inner = inside_rect(
            x,
            y,
            (body.0 + 0.09, body.1 + 0.09, body.2 - 0.18, body.3 - 0.18),
            0.09,
        );
        if outer && !inner {
            return Some(rim);
        }
        let well = (
            body.0 + 0.17,
            body.1 + 0.17,
            (body.2 - 0.34) * level,
            body.3 - 0.34,
        );
        if inside_rect(x, y, well, 0.04) {
            return Some(fill);
        }
        let terminal = (body.0 + body.2 + 0.03, 0.32, 0.13, 0.36);
        inside_rect(x, y, terminal, 0.05).then_some(rim)
    };
    let mut img = Img::new(w, h);
    for py in 0..h {
        for px in 0..w {
            let mut sum = [0f32; 4];
            for k in 0..16 {
                let x = (px as f32 + ((k % 4) as f32 + 0.5) / 4.0) / w as f32 * fw;
                let y = (py as f32 + ((k / 4) as f32 + 0.5) / 4.0) / h as f32 * fh;
                if let Some(c) = colour_at(x, y) {
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += v as f32 / 16.0;
                    }
                }
            }
            let a = sum[3];
            let px_ = &mut img.px[(py * w + px) * 4..][..4];
            for k in 0..3 {
                // Coverage-weighted colour, unpremultiplied.
                px_[k] = if a > 0.0 {
                    (sum[k] * 255.0 / a).min(255.0) as u8
                } else {
                    0
                };
            }
            px_[3] = a as u8;
        }
    }
    img
}

/// An icon `w` by `h` units (1 unit is `px` pixels tall) from `colour_at`, which gives the
/// colour at a point in units, if any: each pixel sampled 16 times, so edges are smooth.
fn sampled(px: usize, (fw, fh): (f32, f32), colour_at: impl Fn(f32, f32) -> Option<Rgba>) -> Img {
    let (w, h) = (
        (fw * px as f32).round() as usize,
        (fh * px as f32).round() as usize,
    );
    let mut img = Img::new(w.max(1), h.max(1));
    for py in 0..h {
        for qx in 0..w {
            let mut sum = [0f32; 4];
            for k in 0..16 {
                let x = (qx as f32 + ((k % 4) as f32 + 0.5) / 4.0) / px as f32;
                let y = (py as f32 + ((k / 4) as f32 + 0.5) / 4.0) / px as f32;
                if let Some(c) = colour_at(x, y) {
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += v as f32 / 16.0;
                    }
                }
            }
            let a = sum[3];
            let out = &mut img.px[(py * w + qx) * 4..][..4];
            for k in 0..3 {
                // Coverage-weighted colour, unpremultiplied.
                out[k] = if a > 0.0 {
                    (sum[k] * 255.0 / a).min(255.0) as u8
                } else {
                    0
                };
            }
            out[3] = a as u8;
        }
    }
    img
}

/// How far a point is from the segment from `a` to `b`.
fn from_segment((x, y): (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a.0 - t * dx).hypot(y - a.1 - t * dy)
}

/// The slash across a radio that is off, from bottom left to top right of a `w`-wide icon,
/// with a dark edge so it reads over the icon under it.
fn slash(x: f32, y: f32, w: f32) -> Option<Rgba> {
    let d = from_segment((x, y), (0.06, 0.94), (w - 0.06, 0.06));
    if d < 0.055 {
        Some([200, 204, 212, 255])
    } else if d < 0.11 {
        Some([20, 24, 32, 255])
    } else {
        None
    }
}

/// The status bar's Wi-Fi: a dot and three arcs over it, the first `lit` of those four (from
/// the dot out) in `on`, the rest in `off`; slashed when the radio is off. `h` pixels tall.
pub fn wifi(h: usize, lit: u8, on: Rgba, off: Rgba, slashed: bool) -> Img {
    const W: f32 = 1.3;
    let (cx, cy) = (W / 2.0, 0.9);
    sampled(h, (W, 1.0), |x, y| {
        if slashed {
            if let Some(c) = slash(x, y, W) {
                return Some(c);
            }
        }
        let (dx, dy) = (x - cx, y - cy);
        let r = dx.hypot(dy);
        // The dot, then the arcs, each a quarter turn wide about straight up.
        let part = if r < 0.11 {
            0
        } else if dy < 0.0 && dx.abs() < -dy && (r - 0.36).abs() < 0.065 {
            1
        } else if dy < 0.0 && dx.abs() < -dy && (r - 0.58).abs() < 0.065 {
            2
        } else if dy < 0.0 && dx.abs() < -dy && (r - 0.8).abs() < 0.065 {
            3
        } else {
            return None;
        };
        Some(if part < lit { on } else { off })
    })
}

/// The status bar's Bluetooth: its rune in `colour`, slashed when the radio is off. `h` pixels
/// tall.
pub fn bluetooth(h: usize, colour: Rgba, slashed: bool) -> Img {
    const W: f32 = 0.72;
    let rune = [
        (0.12, 0.28),
        (0.58, 0.72),
        (0.35, 0.94),
        (0.35, 0.06),
        (0.58, 0.28),
        (0.12, 0.72),
    ];
    sampled(h, (W, 1.0), |x, y| {
        if slashed {
            if let Some(c) = slash(x, y, W) {
                return Some(c);
            }
        }
        rune.windows(2)
            .any(|ab| from_segment((x, y), ab[0], ab[1]) < 0.055)
            .then_some(colour)
    })
}

/// Heart, for favourites.
pub fn heart(size: usize) -> Img {
    shape(size, |x, y| {
        let (x, y) = (x * 1.25, 0.25 - y * 1.25);
        (x * x + y * y - 1.0).powi(3) - x * x * y * y * y < 0.0
    })
}

/// A picture file: a PNG, or a JPEG (a port's screenshot, say).
pub fn load_png(path: &Path) -> Option<Img> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 2];
    std::io::Read::read_exact(&mut file, &mut magic).ok()?;
    if magic == [0xFF, 0xD8] {
        return decode_jpeg(&std::fs::read(path).ok()?);
    }
    std::io::Seek::rewind(&mut file).ok()?;
    decode_png(file)
}

/// A JPEG's pixels, as RGBA.
pub fn decode_jpeg(bytes: &[u8]) -> Option<Img> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
    let px = dec.decode().ok()?;
    let (w, h) = dec.dimensions()?;
    (px.len() == w * h * 4).then_some(Img { w, h, px })
}

pub fn decode_png(source: impl std::io::Read) -> Option<Img> {
    let mut dec = png::Decoder::new(source);
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let n = w * h;
    let mut img = Img::new(w, h);
    let src = &buf[..info.buffer_size()];
    for i in 0..n {
        let p: Rgba = match info.color_type {
            png::ColorType::Rgba => [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]],
            png::ColorType::Rgb => [src[i * 3], src[i * 3 + 1], src[i * 3 + 2], 255],
            png::ColorType::Grayscale => [src[i], src[i], src[i], 255],
            png::ColorType::GrayscaleAlpha => [src[i * 2], src[i * 2], src[i * 2], src[i * 2 + 1]],
            png::ColorType::Indexed => return None,
        };
        img.px[i * 4..i * 4 + 4].copy_from_slice(&p);
    }
    Some(img)
}

pub fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

pub struct Fonts {
    regular: fontdue::Font,
    bold: fontdue::Font,
}

/// CJK fallback, loaded from the system when present (platform::FALLBACK_FONTS; the embedded
/// faces are Latin only).
/// Reading it takes about a second there and holds up everything else being read from the
/// card, so it is left until the shelf is on screen; only text that needs it sooner waits.
fn fallback() -> Option<&'static fontdue::Font> {
    static FALLBACK: std::sync::OnceLock<Option<fontdue::Font>> = std::sync::OnceLock::new();
    FALLBACK
        .get_or_init(|| {
            crate::platform::FALLBACK_FONTS.iter().find_map(|p| {
                let bytes = std::fs::read(p).ok()?;
                fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
            })
        })
        .as_ref()
}

/// Reads the fallback font in the background, ahead of the first title that wants it.
pub fn warm_fallback() {
    std::thread::spawn(fallback);
}

impl Fonts {
    pub fn load() -> Fonts {
        let mk = |b: &[u8]| {
            fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).expect("embedded font")
        };
        Fonts {
            regular: mk(include_bytes!("../assets/RobotoCondensed-Regular.ttf")),
            bold: mk(include_bytes!("../assets/RobotoCondensed-Bold.ttf")),
        }
    }

    fn pick(&self, c: char, bold: bool) -> &fontdue::Font {
        let main = if bold { &self.bold } else { &self.regular };
        if main.lookup_glyph_index(c) == 0 {
            if let Some(fb) = fallback() {
                return fb;
            }
        }
        main
    }

    pub fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        s.chars()
            .map(|c| self.pick(c, bold).metrics(c, size).advance_width)
            .sum()
    }

    /// Render one line as white-on-transparent. Returns an image at least 1x1.
    pub fn render(&self, s: &str, size: f32, bold: bool) -> Img {
        let main = if bold { &self.bold } else { &self.regular };
        let lm = main.horizontal_line_metrics(size);
        let (asc, desc) = lm
            .map(|m| (m.ascent, m.descent))
            .unwrap_or((size * 0.9, -size * 0.25));
        let w = self.width(s, size, bold).ceil() as usize + 2;
        let h = (asc - desc).ceil() as usize + 2;
        let mut img = Img::new(w.max(1), h.max(1));
        let mut pen = 1.0f32;
        for c in s.chars() {
            let (m, bitmap) = self.pick(c, bold).rasterize(c, size);
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (asc + 1.0).round() as i32 - m.height as i32 - m.ymin;
            for y in 0..m.height {
                for x in 0..m.width {
                    let a = bitmap[y * m.width + x];
                    if a > 0 {
                        img.over(
                            gx + x as i32,
                            gy + y as i32,
                            [255, 255, 255, 255],
                            a as f32 / 255.0,
                        );
                    }
                }
            }
            pen += m.advance_width;
        }
        img
    }

    /// `s` broken at spaces into lines that fit `width`, at most `most` of them; the last one
    /// that had to be cut ends in an ellipsis. A word wider than `width` is cut too.
    pub fn wrap(&self, s: &str, size: f32, bold: bool, width: f32, most: usize) -> Vec<String> {
        let fits = |t: &str| self.width(t, size, bold) <= width;
        let mut lines: Vec<String> = Vec::new();
        for word in s.split_whitespace() {
            let full = lines.len() == most;
            match lines.last_mut() {
                Some(line) if fits(&format!("{line} {word}")) => {
                    line.push(' ');
                    line.push_str(word);
                }
                Some(last) if full => {
                    *last = self.ellipsize(last, size, bold, width, true);
                    return lines;
                }
                _ => lines.push(self.ellipsize(word, size, bold, width, false)),
            }
        }
        lines
    }

    /// `s` as it is if it fits `width`, or cut short to fit with an ellipsis (always when
    /// `more`: there is more after it).
    fn ellipsize(&self, s: &str, size: f32, bold: bool, width: f32, more: bool) -> String {
        if !more && self.width(s, size, bold) <= width {
            return s.to_owned();
        }
        let mut t: String = s.to_owned();
        loop {
            let cut = format!("{}\u{2026}", t.trim_end());
            if t.is_empty() || self.width(&cut, size, bold) <= width {
                return cut;
            }
            t.pop();
        }
    }

    /// Largest size in `max..=min` (stepping down) at which `s` fits `width`.
    pub fn fit(&self, s: &str, max: f32, min: f32, width: f32, bold: bool) -> f32 {
        let mut size = max;
        while size > min && self.width(s, size, bold) > width {
            size -= 2.0;
        }
        size.max(min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping() {
        let fonts = Fonts::load();
        let text =
            "Both screens, the gamepad, touch and sound, in a plain app made from the template";
        let w = fonts.width("Both screens, the gamepad,", 26.0, false) + 1.0;
        let lines = fonts.wrap(text, 26.0, false, w, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Both screens, the gamepad,");
        assert!(lines[1].ends_with('\u{2026}'), "{lines:?}");
        assert!(lines.iter().all(|l| fonts.width(l, 26.0, false) <= w));
        assert_eq!(fonts.wrap("Short", 26.0, false, w, 2), ["Short"]);
        assert!(fonts.wrap("", 26.0, false, w, 2).is_empty());
    }

    /// How far right the fill reaches along the icon's middle row, as a fraction of its width.
    fn filled(img: &Img) -> f32 {
        let y = img.h / 2;
        let lit = (0..img.w)
            .filter(|&x| {
                let [r, g, b, a] = img.get(x, y);
                a > 200 && (r, g, b) == (235, 238, 244)
            })
            .max();
        lit.map_or(0.0, |x| x as f32 / img.w as f32)
    }

    #[test]
    fn battery_icon() {
        let full = battery(40, 100, Mark::None);
        assert_eq!((full.w, full.h), (84, 40));
        let (half, low) = (battery(40, 50, Mark::None), battery(40, 5, Mark::None));
        assert!(
            filled(&full) > 0.75 && (0.4..0.55).contains(&filled(&half)),
            "{}",
            filled(&half)
        );
        // Low and not charging: red.
        assert_eq!(filled(&low), 0.0);
        assert!((0..low.w).any(|x| low.get(x, 20)[..3] == [226, 70, 70]));
        // Charging: the bolt's yellow, over the middle.
        let charging = battery(40, 50, Mark::Charging);
        assert_eq!(charging.get(charging.w * 43 / 100, 20)[..3], [255, 206, 72]);
    }
}
