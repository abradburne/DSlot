//! The View sheet (Start): the menu's View page, how the shelf's page is shown, drawn as a panel
//! on the top screen in the game's details' place, so that the shelf it changes stays in sight on
//! the touch screen. The rows and what they do are the menu's (menu.rs, `Page::View`); this
//! only draws them.

use super::draw::{Ui, DIM, FAINT, PANEL, WHITE};
use super::layout::*;
use super::menu::{Shown, Value};

/// The panel: in the details' column, beside the box, centred down the screen.
const X: f32 = 560.0;
const W: f32 = SW - 20.0 - X;
const PAD: f32 = 24.0;
const ROW_H: f32 = 64.0;

impl Ui<'_> {
    /// The View page's title, rows and hint, in a panel on the top screen.
    pub fn draw_sheet(&mut self, sheet: &Shown) -> Result<(), String> {
        let v = self.top;
        // Drawn after the shelf, which leaves the canvas clipped to the touch screen.
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let hint = self.fonts.wrap(&sheet.hint, 17.0, false, W - 2.0 * PAD, 2);
        let h =
            PAD + 40.0 + sheet.rows.len() as f32 * ROW_H + 12.0 + hint.len() as f32 * 24.0 + PAD;
        self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
        self.white[v.c].set_alpha_mod(255);
        let top = ((SH - h) / 2.0).max(20.0);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(X, top, W, h))?;
        let (x, right) = (X + PAD, X + W - PAD);
        self.text(v, &sheet.title, (22.0, true), (x, top + PAD), 0.0, WHITE)?;
        let mut y = top + PAD + 40.0;
        for (r, row) in sheet.rows.iter().enumerate() {
            let on = r == sheet.row;
            if on {
                self.white[v.c].set_color_mod(255, 255, 255);
                self.white[v.c].set_alpha_mod(28);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(X + 8.0, y, W - 16.0, ROW_H - 6.0),
                )?;
            }
            self.text(v, &row.label, (22.0, false), (x, y + 10.0), 0.0, DIM)?;
            let (text, inherited) = value_text(&row.value);
            // The arrows say left and right change it, on the row they would.
            let text = if on {
                format!("\u{2039}  {text}  \u{203a}")
            } else {
                text
            };
            let colour = if inherited { DIM } else { WHITE };
            self.text(v, &text, (22.0, true), (right, y + 8.0), 1.0, colour)?;
            if let Some(source) = &row.source {
                self.text(v, source, (16.0, false), (right, y + 36.0), 1.0, FAINT)?;
            }
            y += ROW_H;
        }
        y += 12.0;
        for line in hint {
            self.text(v, &line, (17.0, false), (x, y), 0.0, FAINT)?;
            y += 24.0;
        }
        Ok(())
    }
}

/// A row's value as words, and whether it is inherited (muted).
fn value_text(value: &Value) -> (String, bool) {
    match value {
        Value::Text(t) => (t.clone(), false),
        Value::Inherited(t) => (t.clone(), true),
        Value::Status { text, .. } | Value::Prompt { text, .. } => (text.clone(), false),
        Value::Stars(n) => ("\u{2605}".repeat(*n as usize), false),
        Value::Slider(p) => (format!("{p}%"), false),
    }
}
