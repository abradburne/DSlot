//! The shelf's layout with nothing in it, drawn in software: at power-on it goes straight on
//! both screens through KMS (kms.rs) while the games are listed and SDL and GL start, and the
//! shelf then comes up over it. `dslot --skeleton-shot` writes it to a PNG.

use super::draw::{slot_bar, FAINT, PANEL};
use super::layout::{BAR_H, CART_SIZE, CART_Y, SH, STEP, SW};
use crate::art::{Slot, CART_H, CART_W, CASE_SIZE};
use crate::gfx::{Fonts, Img};

/// The top screen and the bottom one.
pub fn skeleton(fonts: &Fonts) -> (Img, Img) {
    let (w, h) = (SW as usize, SH as usize);
    let mut top = Img::new(w, h);
    let mut bot = Img::new(w, h);
    for img in [&mut top, &mut bot] {
        for px in img.px.chunks_exact_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
    }
    let panel = [PANEL.r, PANEL.g, PANEL.b, 255];
    let line = [40, 46, 60, 255];

    // Top: the box, the title and the play-time rows.
    let (cw, ch) = (CASE_SIZE.0 as f32, CASE_SIZE.1 as f32);
    top.rrect(
        300.0 - cw / 2.0,
        SH / 2.0 - ch / 2.0,
        cw,
        ch,
        [14.0; 4],
        panel,
    );
    let x = 580.0;
    top.rrect(x, 236.0, 320.0, 44.0, [8.0; 4], line);
    top.rrect(x, 304.0, 200.0, 22.0, [6.0; 4], line);
    top.rrect(x, 346.0, 140.0, 18.0, [6.0; 4], line);
    for i in 0..4 {
        let y = 452.0 + i as f32 * 36.0;
        top.rrect(x, y, 96.0, 18.0, [6.0; 4], panel);
        top.rrect(x + 150.0, y, 72.0, 18.0, [6.0; 4], line);
    }

    // Bottom: carts over the slot, the heading saying what is going on, the status bar.
    for d in [-1.0f32, 1.0, 0.0] {
        let away = d.abs().min(1.0);
        let cw = CART_SIZE * (1.0 - 0.3 * away);
        let chh = cw * CART_H as f32 / CART_W as f32;
        let c = if d == 0.0 { line } else { panel };
        bot.rrect(
            SW / 2.0 + d * STEP - cw / 2.0,
            CART_Y - chh / 2.0,
            cw,
            chh,
            [cw * 0.05, cw * 0.05, cw * 0.02, cw * 0.09],
            c,
        );
    }
    let text = fonts.render("Looking for games\u{2026}", 20.0, false);
    bot.stamp(
        &text,
        (SW / 2.0 - text.w as f32 / 2.0) as i32,
        24,
        [FAINT.r, FAINT.g, FAINT.b, 255],
    );
    let bar = slot_bar(1.0, Slot::One, 0.0);
    for y in 0..bar.h {
        for x in 0..bar.w {
            let p = bar.get(x, y);
            bot.over(x as i32, (SH - BAR_H) as i32 + y as i32, p, 1.0);
        }
    }
    (top, bot)
}
