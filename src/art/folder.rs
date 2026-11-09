//! A folder of games' picture, as its cart on the shelf and its box on the top screen: a card
//! folder in its console's colour, the boxes of the first games in it standing up out of it,
//! and its name and how many games it holds on the front.

use crate::gfx::{Fonts, Img, Rgba};
use crate::rom::Game;

/// How wide a folder is for its height.
const ASPECT: f32 = 1.02;

/// `a` a share `k` of the way to `b`.
fn mix(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k)
}

fn rgba(c: [f32; 3], alpha: u8) -> Rgba {
    [
        c[0].clamp(0.0, 255.0) as u8,
        c[1].clamp(0.0, 255.0) as u8,
        c[2].clamp(0.0, 255.0) as u8,
        alpha,
    ]
}

/// Whether `(x, y)` is inside the rectangle `(rx, ry, rw, rh)` with corners rounded `r`
/// (top-left, top-right, bottom-right, bottom-left).
fn in_rrect(x: f32, y: f32, (rx, ry, rw, rh): (f32, f32, f32, f32), r: [f32; 4]) -> bool {
    if x < rx || y < ry || x > rx + rw || y > ry + rh {
        return false;
    }
    let (cx, cy) = (rx + rw / 2.0, ry + rh / 2.0);
    let rad = match (x < cx, y < cy) {
        (true, true) => r[0],
        (false, true) => r[1],
        (false, false) => r[2],
        (true, false) => r[3],
    };
    let qx = (x - cx).abs() - rw / 2.0 + rad;
    let qy = (y - cy).abs() - rh / 2.0 + rad;
    qx <= 0.0 || qy <= 0.0 || qx.hypot(qy) <= rad
}

/// Fills the shape `inside` says, within `bounds` (x0, y0, x1, y1), anti-aliased (4 x 4
/// samples a pixel), each row its colour from `colour` (0 at `bounds`' top, 1 at its bottom).
fn fill(
    img: &mut Img,
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    inside: impl Fn(f32, f32) -> bool,
    colour: impl Fn(f32) -> Rgba,
) {
    let (ya, yb) = (y0.floor().max(0.0) as i32, y1.ceil() as i32);
    let (xa, xb) = (x0.floor().max(0.0) as i32, x1.ceil() as i32);
    for py in ya..yb {
        let c = colour(((py as f32 + 0.5 - y0) / (y1 - y0)).clamp(0.0, 1.0));
        for px in xa..xb {
            let mut n = 0;
            for k in 0..16 {
                let sx = px as f32 + (k % 4) as f32 / 4.0 + 0.125;
                let sy = py as f32 + (k / 4) as f32 / 4.0 + 0.125;
                n += inside(sx, sy) as u32;
            }
            if n > 0 {
                img.over(px, py, c, n as f32 / 16.0);
            }
        }
    }
}

/// The folder, `w` x `h`, standing at the bottom of it. `colour` is its console's; `inside`
/// the pictures standing up out of it (the folder's own picture, or its first games' boxes),
/// the first at the front.
pub fn draw(
    game: &Game,
    fonts: &Fonts,
    colour: [u8; 3],
    inside: &[Img],
    w: usize,
    h: usize,
) -> Img {
    let mut img = Img::new(w, h);
    let (wf, hf) = (w as f32, h as f32);
    // The folder as large as fits at its shape, on the bottom edge, centred across.
    let fw = wf.min(hf * ASPECT) * 0.96;
    let fh = fw / ASPECT;
    let (x, y) = ((wf - fw) / 2.0, hf - fh - hf * 0.01);
    let u = fw / 100.0;
    let base = colour.map(f32::from);
    let white = [255.0; 3];
    let black = [0.0; 3];
    // Its shades: the back darker, the front lighter at the top where the light catches it.
    let back_top = mix(mix(base, black, 0.35), white, 0.04);
    let back_bottom = mix(base, black, 0.55);
    let front_top = mix(base, white, 0.30);
    let front_bottom = mix(base, black, 0.10);
    let rim = mix(base, white, 0.55);

    // The back, its tab up at the left, the tab's right side sloping down to the back's top.
    let back_y = y + 10.0 * u;
    let (tab_x, tab_w, tab_h, slope) = (x + 4.0 * u, 34.0 * u, 10.0 * u, 7.0 * u);
    let r = 5.0 * u;
    let back = (x, back_y, fw, fh - 10.0 * u);
    let tab = move |px: f32, py: f32| {
        if py < y || py > back_y + r || px < tab_x {
            return false;
        }
        // The sloping right edge, and the top-left corner rounded.
        let edge = tab_x + tab_w + slope * ((py - y) / tab_h).clamp(0.0, 1.0);
        px <= edge
            && in_rrect(
                px,
                py,
                (tab_x, y, tab_w + slope, tab_h * 2.0),
                [r * 0.6, 0.0, 0.0, 0.0],
            )
    };
    fill(
        &mut img,
        (x, y, x + fw, y + fh),
        |px, py| in_rrect(px, py, back, [r * 0.6, r * 0.6, r, r]) || tab(px, py),
        |t| rgba(mix(back_top, back_bottom, t), 255),
    );

    // What is in it, standing up out of it: the boxes of its first games, fanned a little, the
    // first in the middle and in front. Each is cut off where the front covers it.
    let front_y = y + fh * 0.50;
    let slots: [(f32, f32); 3] = [(50.0, 2.0), (28.0, 9.0), (72.0, 12.0)];
    let mut drawn: Vec<(usize, (f32, f32))> = inside
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, _)| (i, slots[i]))
        .collect();
    // With one alone, in the middle; with two, either side of it.
    if drawn.len() == 2 {
        drawn[0].1 = (38.0, 6.0);
        drawn[1].1 = (62.0, 10.0);
    }
    for &(i, (cx, top)) in drawn.iter().rev() {
        let pic = &inside[i];
        let pw = 44.0 * u;
        let ph = (pw * pic.h as f32 / pic.w.max(1) as f32).min(fh * 0.9);
        let (px, py) = (x + cx * u - pw / 2.0, y + top * u);
        let shown = (front_y - py + 2.0 * u).min(ph).max(0.0);
        if shown <= 0.0 {
            continue;
        }
        // Its shadow on what is behind it.
        img.rrect(
            px + 1.5 * u,
            py + 1.5 * u,
            pw,
            shown,
            [1.5 * u; 4],
            [0, 0, 0, 90],
        );
        let cut = shown / ph * pic.h as f32;
        img.blit(
            pic,
            (0.0, 0.0, pic.w as f32, cut),
            (px as i32, py as i32, pw as i32, shown.round() as i32),
            false,
        );
        // A rim of light round its top.
        img.rrect(px, py, pw, 0.5 * u, [0.0; 4], [255, 255, 255, 50]);
    }

    // The front's shadow, cast up over what stands in the folder.
    let shade_h = 6.0 * u;
    fill(
        &mut img,
        (x, front_y - shade_h, x + fw, front_y),
        |px, py| px >= x + r * 0.5 && px <= x + fw - r * 0.5 && py >= front_y - shade_h,
        |t| [0, 0, 0, (t * t * 110.0) as u8],
    );
    // The front: lighter at the top, a rim of light along its top edge.
    let front = (x, front_y, fw, y + fh - front_y);
    let fr = [r * 0.5, r * 0.5, r, r];
    fill(
        &mut img,
        (x, front_y, x + fw, y + fh),
        |px, py| in_rrect(px, py, front, fr),
        |t| rgba(mix(front_top, front_bottom, t.powf(0.8)), 255),
    );
    fill(
        &mut img,
        (x, front_y, x + fw, front_y + 0.9 * u),
        |px, py| in_rrect(px, py, front, fr),
        |_| rgba(rim, 200),
    );
    // A fold pressed across it near the bottom.
    let fold = y + fh - 9.0 * u;
    img.rrect(
        x + 3.0 * u,
        fold,
        fw - 6.0 * u,
        0.6 * u,
        [0.0; 4],
        [0, 0, 0, 30],
    );
    img.rrect(
        x + 3.0 * u,
        fold + 0.6 * u,
        fw - 6.0 * u,
        0.5 * u,
        [0.0; 4],
        [255, 255, 255, 30],
    );

    // The name, and how many games, on the front.
    let room = fw - 14.0 * u;
    let area = (y + fh - 9.0 * u) - front_y;
    let size = fonts.fit(game.name(), area * 0.36, 9.0, room, true);
    let name = fonts.render(game.name(), size, true);
    let count = game.title.get(1).map(String::as_str).unwrap_or("");
    let small = (size * 0.5).max(8.0);
    let under = fonts.render(count, small, false);
    let gap = small * 0.35;
    let block = name.h as f32 + gap + under.h as f32;
    let ty = front_y + (area - block) / 2.0;
    let nx = (x + (fw - name.w as f32) / 2.0) as i32;
    img.stamp(
        &name,
        nx,
        (ty + 0.25 * u.max(1.0)) as i32 + 1,
        [0, 0, 0, 90],
    );
    img.stamp(&name, nx, ty as i32, [255, 255, 255, 255]);
    img.stamp(
        &under,
        (x + (fw - under.w as f32) / 2.0) as i32,
        (ty + name.h as f32 + gap) as i32,
        [255, 255, 255, 175],
    );
    img
}
