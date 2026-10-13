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
        Img { w, h, px: vec![0; w * h * 4] }
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
        for k in 0..3 {
            let v = (c[k] as f32 * sa + self.px[i + k] as f32 * da * (1.0 - sa)) / oa;
            self.px[i + k] = v.round() as u8;
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
    pub fn blit(&mut self, src: &Img, s: (f32, f32, f32, f32), d: (i32, i32, i32, i32), nearest: bool) {
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
        [(acc[0] / acc[3]) as u8, (acc[1] / acc[3]) as u8, (acc[2] / acc[3]) as u8, (acc[3] / n) as u8]
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
            let at = |v: usize, k: usize| (v as f32 + (k as f32 + 0.5) / 4.0) / size as f32 * 2.0 - 1.0;
            let hits = (0..16).filter(|k| inside(at(x, k % 4), at(y, k / 4))).count();
            img.px[(y * size + x) * 4..][..4].copy_from_slice(&[255, 255, 255, (hits * 255 / 16) as u8]);
        }
    }
    img
}

/// Five-pointed star, for ratings.
pub fn star(size: usize) -> Img {
    use std::f32::consts::PI;
    let points: Vec<(f32, f32)> = (0..10).map(|i| {
        let (a, r) = (i as f32 * PI / 5.0, if i % 2 == 0 { 1.0 } else { 0.42 });
        (r * a.sin(), 0.05 - r * a.cos())
    }).collect();
    shape(size, |x, y| {
        // Even-odd crossing test against the outline.
        (0..10).filter(|&i| {
            let ((ax, ay), (bx, by)) = (points[i], points[(i + 1) % 10]);
            (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * (bx - ax)
        }).count() % 2 == 1
    })
}

/// Heart, for favourites.
pub fn heart(size: usize) -> Img {
    shape(size, |x, y| {
        let (x, y) = (x * 1.25, 0.25 - y * 1.25);
        (x * x + y * y - 1.0).powi(3) - x * x * y * y * y < 0.0
    })
}

pub fn load_png(path: &Path) -> Option<Img> {
    decode_png(File::open(path).ok()?)
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
    /// CJK fallback, loaded from the device when present (the embedded faces are Latin only).
    fallback: Option<fontdue::Font>,
}

const FALLBACK_FONTS: [&str; 2] = [
    "/mnt/vendor/deep/nnddss/assets/fonts/SourceHanSansSC-Normal.ttf",
    "/mnt/vendor/bin/default.ttf",
];

impl Fonts {
    pub fn load() -> Fonts {
        let mk = |b: &[u8]| fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).expect("embedded font");
        let fallback = FALLBACK_FONTS.iter().find_map(|p| {
            let bytes = std::fs::read(p).ok()?;
            fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
        });
        Fonts {
            regular: mk(include_bytes!("../assets/RobotoCondensed-Regular.ttf")),
            bold: mk(include_bytes!("../assets/RobotoCondensed-Bold.ttf")),
            fallback,
        }
    }

    fn pick(&self, c: char, bold: bool) -> &fontdue::Font {
        let main = if bold { &self.bold } else { &self.regular };
        match &self.fallback {
            Some(fb) if main.lookup_glyph_index(c) == 0 => fb,
            _ => main,
        }
    }

    pub fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        s.chars().map(|c| self.pick(c, bold).metrics(c, size).advance_width).sum()
    }

    /// Render one line as white-on-transparent. Returns an image at least 1x1.
    pub fn render(&self, s: &str, size: f32, bold: bool) -> Img {
        let main = if bold { &self.bold } else { &self.regular };
        let lm = main.horizontal_line_metrics(size);
        let (asc, desc) = lm.map(|m| (m.ascent, m.descent)).unwrap_or((size * 0.9, -size * 0.25));
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
                        img.over(gx + x as i32, gy + y as i32, [255, 255, 255, 255], a as f32 / 255.0);
                    }
                }
            }
            pen += m.advance_width;
        }
        img
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
