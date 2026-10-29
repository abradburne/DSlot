//! SDL's events as the shelf understands them: the DS+'s buttons by what is printed on them
//! (nibkit::input::printed), the keys that stand in for them on a keyboard, and the mouse as
//! the touch screen in a desktop preview.

use super::layout::{View, SH, SW};
use crate::platform;
use nibkit::input::Face;
use nibkit::touch::Pointer;
use sdl2::controller::{Axis, Button};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

/// A button of the DS+.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    Start,
    L,
    R,
    /// The triggers behind the shoulders, which SDL reads as axes: pushed in past a point,
    /// pressed.
    L2,
    R2,
    /// Select tapped alone (held with Start, it leaves DSlot).
    Select,
    /// The Back key beside Menu: a key of its own, not a gamepad button (nibOS's
    /// docs/engineering/input.md).
    Back,
}

#[derive(Debug, PartialEq)]
pub enum Input {
    Press(Key),
    Release(Key),
    /// A touch on the touch screen, in layout units.
    Pointer(Pointer),
    /// Leave DSlot: Select + Start, Escape, the window closed (or Menu, on the stock OS).
    Quit,
}

/// Reads SDL's events, remembering whether Select is held (and whether Start went down with
/// it) and where the stick is pushed.
#[derive(Default)]
pub struct Reader {
    select: bool,
    chord: bool,
    /// The direction the left stick is pushed along each axis (x, y), as a D-pad press.
    stick: [Option<Key>; 2],
    /// Whether each trigger (L2, R2) is pressed in.
    triggers: [bool; 2],
}

/// How far the stick is pushed (of 32767) to press a direction, and let back to let it go.
const STICK_PRESS: i16 = 16000;
const STICK_LET_GO: i16 = 8000;

impl Reader {
    /// `mouse` is the touch screen's view when the mouse stands in for the touch panel, `None`
    /// when the panel is read directly (whose touches SDL also reports as the mouse).
    pub fn read(&mut self, event: Event, mouse: Option<View>) -> Option<Input> {
        match event {
            Event::Quit { .. }
            | Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => Some(Input::Quit),
            // Held keys repeat on the shelf's own clock, as the D-pad does.
            Event::KeyDown { repeat: true, .. } => None,
            Event::KeyDown {
                keycode: Some(key), ..
            } => keyboard(key).map(Input::Press),
            Event::KeyUp {
                keycode: Some(key), ..
            } => keyboard(key).map(Input::Release),
            Event::ControllerButtonDown { button, .. } => match button {
                Button::Back => {
                    (self.select, self.chord) = (true, false);
                    None
                }
                Button::Start if self.select => {
                    self.chord = true;
                    Some(Input::Quit)
                }
                Button::Guide if platform::MENU_QUITS => Some(Input::Quit),
                button => pad(button).map(Input::Press),
            },
            Event::ControllerButtonUp { button, .. } => {
                if button == Button::Back {
                    // Tapped alone, Select is a press of its own, sent as it is let go.
                    let alone = self.select && !self.chord;
                    self.select = false;
                    return alone.then_some(Input::Press(Key::Select));
                }
                pad(button).map(Input::Release)
            }
            // The left stick as the D-pad: pushed past a point, a direction goes down (and
            // repeats while held); let back nearer the middle, it comes up.
            // A trigger, past the same points as the stick.
            Event::ControllerAxisMotion {
                axis: axis @ (Axis::TriggerLeft | Axis::TriggerRight),
                value,
                ..
            } => {
                let (i, key) = match axis {
                    Axis::TriggerLeft => (0, Key::L2),
                    _ => (1, Key::R2),
                };
                let was = self.triggers[i];
                let now = match value {
                    v if v >= STICK_PRESS => true,
                    v if v < STICK_LET_GO => false,
                    _ => was,
                };
                self.triggers[i] = now;
                match (was, now) {
                    (false, true) => Some(Input::Press(key)),
                    (true, false) => Some(Input::Release(key)),
                    _ => None,
                }
            }
            Event::ControllerAxisMotion { axis, value, .. } => {
                let (i, keys) = match axis {
                    Axis::LeftX => (0, [Key::Left, Key::Right]),
                    Axis::LeftY => (1, [Key::Up, Key::Down]),
                    _ => return None,
                };
                let now = match value {
                    v if v <= -STICK_PRESS => Some(keys[0]),
                    v if v >= STICK_PRESS => Some(keys[1]),
                    v if v.unsigned_abs() < STICK_LET_GO as u16 => None,
                    _ => return None,
                };
                match (self.stick[i], now) {
                    (was, now) if was == now => None,
                    // Let go (or swung straight over: let go first, pressed on the next event).
                    (Some(was), _) => {
                        self.stick[i] = None;
                        Some(Input::Release(was))
                    }
                    (None, Some(key)) => {
                        self.stick[i] = Some(key);
                        Some(Input::Press(key))
                    }
                    (None, None) => None,
                }
            }
            Event::MouseButtonDown { x, y, .. } => {
                let (x, y) = mouse?.unscale(x, y);
                ((0.0..SW).contains(&x) && (0.0..SH).contains(&y))
                    .then_some(Input::Pointer(Pointer::Down(x, y)))
            }
            Event::MouseMotion { x, y, .. } => {
                let (x, y) = mouse?.unscale(x, y);
                Some(Input::Pointer(Pointer::Move(x, y)))
            }
            Event::MouseButtonUp { .. } => mouse.map(|_| Input::Pointer(Pointer::Up)),
            _ => None,
        }
    }
}

/// A face button by the label printed on it (nibkit::input::printed: SDL names them by
/// position); any other button as it is.
fn printed(button: Button) -> Button {
    let face = match button {
        Button::A => Face::A,
        Button::B => Face::B,
        Button::X => Face::X,
        Button::Y => Face::Y,
        other => return other,
    };
    match nibkit::input::printed(face) {
        Face::A => Button::A,
        Face::B => Button::B,
        Face::X => Button::X,
        Face::Y => Button::Y,
    }
}

/// A gamepad button, the face buttons by what is printed on them.
fn pad(button: Button) -> Option<Key> {
    Some(match printed(button) {
        Button::DPadUp => Key::Up,
        Button::DPadDown => Key::Down,
        Button::DPadLeft => Key::Left,
        Button::DPadRight => Key::Right,
        Button::A => Key::A,
        Button::B => Key::B,
        Button::X => Key::X,
        Button::Y => Key::Y,
        Button::Start => Key::Start,
        Button::LeftShoulder => Key::L,
        Button::RightShoulder => Key::R,
        _ => return None,
    })
}

/// The keys that stand in for the buttons in a desktop preview, and the DS+'s Back key, which
/// is one: SDL3 calls it AC Back, and sdl2-compat hands SDL3's number for it to SDL2 programs
/// untranslated, where it is Sleep.
fn keyboard(key: Keycode) -> Option<Key> {
    Some(match key {
        Keycode::AcBack | Keycode::Sleep | Keycode::Backquote => Key::Back,
        Keycode::Up => Key::Up,
        Keycode::Down => Key::Down,
        Keycode::Left => Key::Left,
        Keycode::Right => Key::Right,
        Keycode::Return | Keycode::Space => Key::A,
        Keycode::Backspace => Key::B,
        Keycode::Tab => Key::X,
        Keycode::F => Key::Y,
        Keycode::L | Keycode::PageUp => Key::L,
        Keycode::R | Keycode::PageDown => Key::R,
        Keycode::LeftBracket => Key::L2,
        Keycode::RightBracket => Key::R2,
        Keycode::S => Key::Select,
        Keycode::D => Key::Start,
        _ => return None,
    })
}

/// A key pressed and let go, as SDL would report them, for `--press`.
pub fn scripted(name: &str) -> (Event, Event) {
    let (keycode, keymod) = (Keycode::from_name(name), sdl2::keyboard::Mod::NOMOD);
    let down = Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode,
        scancode: None,
        keymod,
        repeat: false,
    };
    let up = Event::KeyUp {
        timestamp: 0,
        window_id: 0,
        keycode,
        scancode: None,
        keymod,
        repeat: false,
    };
    (down, up)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(button: Button) -> Event {
        Event::ControllerButtonDown {
            timestamp: 0,
            which: 0,
            button,
        }
    }

    fn up(button: Button) -> Event {
        Event::ControllerButtonUp {
            timestamp: 0,
            which: 0,
            button,
        }
    }

    #[test]
    fn select_start_quits() {
        let mut reader = Reader::default();
        assert_eq!(
            reader.read(down(Button::Start), None),
            Some(Input::Press(Key::Start))
        );
        assert_eq!(reader.read(down(Button::Back), None), None);
        assert_eq!(reader.read(down(Button::Start), None), Some(Input::Quit));
        reader.read(up(Button::Back), None);
        assert_eq!(
            reader.read(down(Button::Start), None),
            Some(Input::Press(Key::Start))
        );
    }

    fn key(keycode: Keycode) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(keycode),
            scancode: None,
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        }
    }

    #[test]
    fn the_back_key() {
        let mut reader = Reader::default();
        // As SDL3 names it, as sdl2-compat passes it on, and the desktop preview's stand-in.
        for k in [Keycode::AcBack, Keycode::Sleep, Keycode::Backquote] {
            assert_eq!(reader.read(key(k), None), Some(Input::Press(Key::Back)));
        }
    }

    #[test]
    fn stick() {
        let mut reader = Reader::default();
        let push = |value| Event::ControllerAxisMotion {
            timestamp: 0,
            which: 0,
            axis: Axis::LeftY,
            value,
        };
        // A little push does nothing; past the point, Down; back to the middle, let go.
        assert_eq!(reader.read(push(9000), None), None);
        assert_eq!(
            reader.read(push(20000), None),
            Some(Input::Press(Key::Down))
        );
        assert_eq!(reader.read(push(25000), None), None);
        assert_eq!(reader.read(push(12000), None), None);
        assert_eq!(
            reader.read(push(2000), None),
            Some(Input::Release(Key::Down))
        );
        // Swung straight over to the other side: let go, then pressed that way.
        reader.read(push(30000), None);
        assert_eq!(
            reader.read(push(-30000), None),
            Some(Input::Release(Key::Down))
        );
        assert_eq!(reader.read(push(-30000), None), Some(Input::Press(Key::Up)));
    }

    #[test]
    fn keys() {
        let mut reader = Reader::default();
        let (tab, untab) = scripted("Tab");
        assert_eq!(reader.read(tab, None), Some(Input::Press(Key::X)));
        assert_eq!(reader.read(untab, None), Some(Input::Release(Key::X)));
        assert_eq!(reader.read(scripted("Escape").0, None), Some(Input::Quit));
        assert_eq!(reader.read(scripted("Q").0, None), None);
        // A, B, X and Y on the DS+ are always the same keys, whatever the platform's SDL calls them.
        let printed: Vec<_> = [Button::A, Button::B, Button::X, Button::Y]
            .map(printed)
            .into();
        assert_eq!(printed.len(), 4);
        assert!([Button::A, Button::B, Button::X, Button::Y]
            .iter()
            .all(|b| printed.contains(b)));
    }

    #[test]
    fn mouse_is_the_touch_screen() {
        let mut reader = Reader::default();
        let bot = View {
            ox: 0.0,
            oy: 384.0,
            s: 0.5,
            c: 0,
        };
        let click = |y| Event::MouseButtonDown {
            timestamp: 0,
            window_id: 0,
            which: 0,
            mouse_btn: sdl2::mouse::MouseButton::Left,
            clicks: 1,
            x: 256,
            y,
        };
        assert_eq!(
            reader.read(click(400), Some(bot)),
            Some(Input::Pointer(Pointer::Down(512.0, 32.0)))
        );
        // On the top screen, or with the panel read directly: nothing.
        assert_eq!(reader.read(click(100), Some(bot)), None);
        assert_eq!(reader.read(click(400), None), None);
    }
}
