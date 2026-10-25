//! The power menu: Sleep, Restart, Power off, Cancel. nibOS asks for it on a long press of the
//! power button (nibkit::power). On the shelf it opens at once; if a game was running, DSlot
//! first put it away (it saved its auto-resume state and quit), and Cancel, or waking from
//! Sleep, starts it again, so it carries on where it was.

use super::layout::{SH, SW};

/// What the menu offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Sleep,
    Restart,
    PowerOff,
    Cancel,
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Choice::Sleep => "Sleep",
            Choice::Restart => "Restart",
            Choice::PowerOff => "Power off",
            Choice::Cancel => "Cancel",
        }
    }
}

/// Where the menu's rows are on the touch screen: the first row's middle, and the step.
pub const FIRST: f32 = SH / 2.0 - 80.0;
pub const STEP: f32 = 64.0;
/// The menu's box: its width, centred.
pub const WIDTH: f32 = 520.0;

/// The open power menu.
pub struct PowerMenu {
    /// The highlighted row.
    pub row: usize,
    /// The game put away for the menu, by index into the games, started again on Cancel.
    pub resume: Option<usize>,
    pub choices: Vec<Choice>,
}

impl PowerMenu {
    pub fn open(resume: Option<usize>) -> PowerMenu {
        let sleep = nibkit::device::can_sleep().then_some(Choice::Sleep);
        let choices = sleep
            .into_iter()
            .chain([Choice::Restart, Choice::PowerOff, Choice::Cancel])
            .collect();
        PowerMenu {
            row: 0,
            resume,
            choices,
        }
    }

    /// Moves the highlight by `d` rows, round from the last to the first.
    pub fn moved(&mut self, d: i32) {
        self.row = (self.row as i32 + d).rem_euclid(self.choices.len() as i32) as usize;
    }

    pub fn chosen(&self) -> Choice {
        self.choices[self.row]
    }

    /// The row under a touch, if any.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let inside = (x - SW / 2.0).abs() < WIDTH / 2.0;
        (0..self.choices.len()).find(|&r| inside && (y - row_y(r)).abs() < STEP / 2.0)
    }
}

/// Where row `r` sits: its middle.
pub fn row_y(r: usize) -> f32 {
    FIRST + r as f32 * STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows() {
        let mut menu = PowerMenu::open(Some(3));
        let n = menu.choices.len();
        assert_eq!(menu.choices.last(), Some(&Choice::Cancel));
        menu.moved(-1);
        assert_eq!(menu.chosen(), Choice::Cancel);
        menu.moved(1);
        assert_eq!(menu.row, 0);
        assert_eq!(menu.hit(SW / 2.0, row_y(n - 1) + 10.0), Some(n - 1));
        assert_eq!(menu.hit(10.0, row_y(0)), None);
    }
}
