//! The screensaver's logo, bouncing round both screens as one tall space, its colour changing
//! at every wall it meets. The two screens are stacked with the hinge between them: the logo
//! crosses it as if the screens were one display, out of sight while it does.

use super::layout::{SH, SW};

/// The hinge between the screens, in screen units: the space the logo crosses unseen.
pub const HINGE: f32 = 120.0;
/// The space the logo moves in: the top screen, the hinge, the touch screen.
pub const AREA: (f32, f32) = (SW, SH * 2.0 + HINGE);
/// The logo's size, in screen units.
pub const LOGO: (f32, f32) = (280.0, 128.0);
/// Its speed along each axis, in screen units a second: a slow drift, which looks smooth drawn
/// only a few times a second (shelf.rs, `SAVER_FPS`).
const SPEED: (f32, f32) = (90.0, 66.0);

/// What a step met: a wall, or two at once (a corner).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    None,
    Wall,
    Corner,
}

/// The logo: where it is (its top left corner, in the tall space), which way it goes, its
/// colour (a hue, in turns) and how many walls it has met.
#[derive(Clone, Debug)]
pub struct Bouncer {
    pub x: f32,
    pub y: f32,
    vx: f32,
    vy: f32,
    size: (f32, f32),
    area: (f32, f32),
    pub hue: f32,
    pub bounces: u32,
}

impl Bouncer {
    /// A logo of `size` in `area`, starting at `at` (a fraction of the room it has to move in
    /// along each axis), heading down and to the right.
    pub fn new(area: (f32, f32), size: (f32, f32), at: (f32, f32)) -> Bouncer {
        let room = (area.0 - size.0, area.1 - size.1);
        Bouncer {
            x: room.0 * at.0.clamp(0.0, 1.0),
            y: room.1 * at.1.clamp(0.0, 1.0),
            vx: SPEED.0,
            vy: SPEED.1,
            size,
            area,
            hue: at.0,
            bounces: 0,
        }
    }

    /// Moves it on by `dt` seconds, off any wall it reaches (a step short enough to meet each
    /// wall at most once). A wall turns the colour on a little; a corner, half way round.
    pub fn step(&mut self, dt: f32) -> Hit {
        let room = (self.area.0 - self.size.0, self.area.1 - self.size.1);
        let walls = [
            reflect(&mut self.x, &mut self.vx, dt, room.0),
            reflect(&mut self.y, &mut self.vy, dt, room.1),
        ];
        let hit = match walls {
            [true, true] => Hit::Corner,
            [true, false] | [false, true] => Hit::Wall,
            [false, false] => Hit::None,
        };
        if hit != Hit::None {
            self.bounces += 1;
            self.hue = (self.hue + if hit == Hit::Corner { 0.5 } else { 0.17 }).fract();
        }
        hit
    }
}

/// Moves `pos` by `v * dt` between 0 and `room`, turning back off whichever end it passes.
/// Returns whether it did.
fn reflect(pos: &mut f32, v: &mut f32, dt: f32, room: f32) -> bool {
    *pos += *v * dt;
    if *pos < 0.0 {
        *pos = -*pos;
        *v = v.abs();
        true
    } else if *pos > room {
        *pos = 2.0 * room - *pos;
        *v = -v.abs();
        true
    } else {
        false
    }
}

/// Whether the logo, its top `y` from a screen's top, shows on that screen.
pub fn on_screen(y: f32) -> bool {
    y + LOGO.1 > 0.0 && y < SH
}

/// Which screens the logo is on: the top one, the bottom one (neither while it crosses the
/// hinge).
pub fn screens(b: &Bouncer) -> [bool; 2] {
    [on_screen(b.y), on_screen(b.y - SH - HINGE)]
}

/// A colour of full saturation-ish and brightness for a hue (in turns), as RGB.
pub fn colour(hue: f32) -> (u8, u8, u8) {
    let h = hue.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    // Pastel: mixed a little with white, so the logo's red still shows against its white.
    let c = |v: f32| (255.0 * (0.35 + 0.65 * v)).round() as u8;
    (c(r), c(g), c(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_inside_and_bounces() {
        let mut b = Bouncer::new(AREA, LOGO, (0.3, 0.6));
        let mut hits = 0;
        for _ in 0..30 * 600 {
            if b.step(1.0 / 30.0) != Hit::None {
                hits += 1;
            }
            assert!(b.x >= 0.0 && b.x <= AREA.0 - LOGO.0, "x {}", b.x);
            assert!(b.y >= 0.0 && b.y <= AREA.1 - LOGO.1, "y {}", b.y);
        }
        // Ten minutes: a wall every few seconds, as the speeds and the room say.
        let expected = 600.0 * (SPEED.0 / (AREA.0 - LOGO.0) + SPEED.1 / (AREA.1 - LOGO.1));
        assert!(
            (hits as f32 - expected).abs() <= 3.0,
            "{hits} against {expected}"
        );
        assert_eq!(b.bounces, hits);
    }

    #[test]
    fn walls_and_corners() {
        // Off the right wall: back the other way, the overshoot folded in.
        let mut b = Bouncer::new((100.0, 100.0), (10.0, 10.0), (1.0, 0.5));
        (b.x, b.vx) = (88.0, 150.0);
        assert_eq!(b.step(0.02), Hit::Wall);
        assert!((b.x - (90.0 - (88.0 + 3.0 - 90.0))).abs() < 1e-4, "{}", b.x);
        assert!(b.vx < 0.0);
        let hue = b.hue;

        // Into the bottom right corner: both walls in one step.
        let mut c = Bouncer::new((100.0, 100.0), (10.0, 10.0), (0.0, 0.0));
        (c.x, c.y, c.vx, c.vy) = (89.0, 89.5, 150.0, 110.0);
        c.hue = hue;
        assert_eq!(c.step(0.02), Hit::Corner);
        assert!(c.vx < 0.0 && c.vy < 0.0);
        assert!((c.hue - (hue + 0.5).fract()).abs() < 1e-5);

        // Out of the top left one.
        let mut d = Bouncer::new((100.0, 100.0), (10.0, 10.0), (0.0, 0.0));
        (d.x, d.y, d.vx, d.vy) = (1.0, 0.5, -150.0, -110.0);
        assert_eq!(d.step(0.02), Hit::Corner);
        assert!(d.vx > 0.0 && d.vy > 0.0 && d.x >= 0.0 && d.y >= 0.0);
    }

    #[test]
    fn which_screens() {
        let mut b = Bouncer::new(AREA, LOGO, (0.0, 0.0));
        assert_eq!(screens(&b), [true, false]);
        // Half over the top screen's bottom edge, then in the hinge, then on the touch screen.
        b.y = SH - LOGO.1 / 2.0;
        assert_eq!(screens(&b), [true, false]);
        b.y = SH + 1.0;
        assert_eq!(screens(&b), [false, LOGO.1 > HINGE]);
        b.y = SH + HINGE;
        assert_eq!(screens(&b), [false, true]);
    }

    #[test]
    fn colours() {
        assert_eq!(colour(0.0), (255, 89, 89));
        assert_eq!(colour(1.0 / 3.0), (89, 255, 89));
        assert_eq!(colour(1.25), colour(0.25));
    }
}
